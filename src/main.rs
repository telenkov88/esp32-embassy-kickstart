#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(adt_const_params)]
#![feature(impl_trait_in_assoc_type)]
extern crate alloc;

use crate::ota::{run_with_ota, validate_current_ota_slot};
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::{Spawner, task};
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, Timer};
#[allow(unused_imports)]
use esp_backtrace as _;
use esp_hal::rng::Rng;
use esp_hal::{
    clock::CpuClock,
    system::{CpuControl, Stack},
    timer::{AnyTimer, timg::TimerGroup},
};
use esp_hal::{rmt::Rmt, time::Rate};
use esp_hal_embassy::Executor;
use log::{error, info};
use static_cell::StaticCell;

mod http;
mod main_core;
mod neopixel;
mod second_core;
mod shared;
mod web_server;
mod wifi;

use crate::http::EmbassyHttpClient;
use main_core::enable_disable_led;
use second_core::control_led;
use wifi::init_wifi;

use crate::web_server::AppProps;
use picoserve::{AppBuilder, AppRouter, make_static};
use web_server::web_task;

mod ota;
use embedded_storage::ReadStorage;
use esp_bootloader_esp_idf::ota::Slot;
use esp_bootloader_esp_idf::partitions;
use esp_storage::FlashStorage;
use ota::OtaImageState::Valid;

mod config;
mod db;
mod log_utils;
mod macros;
mod mqtt_config;
mod wifi_config;

use log_utils::log_banner;

use crate::db::DbFlash;
use crate::mqtt_config::{get_default_mqtt_credentials, get_mqtt_credentials};
use crate::wifi::WifiMode;
use crate::wifi_config::{get_default_wifi_credentials, get_wifi_credentials};
use embassy_embedded_hal::adapter::BlockingAsync;
use esp_hal::clock::Clock;
use esp_hal::system::AppCoreGuard;
use heapless::String;

const CONFIG_PARTITION_START: usize = 0xA10000;

static mut APP_CORE_STACK: Stack<8192> = Stack::new();
static CLIENT_STATE: StaticCell<TcpClientState<3, 1024, 1024>> = StaticCell::new();
static TCP_CLIENT: StaticCell<TcpClient<'static, 3>> = StaticCell::new();

pub static WIFI_INITIALIZED: AtomicBool = AtomicBool::new(false);
pub static WIFI_MODE_CLIENT: AtomicBool = AtomicBool::new(false);
pub static TIME_SYNCED: AtomicBool = AtomicBool::new(false);
pub static FIRMWARE_UPGRADE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[task(pool_size = 2)]
pub async fn http_wk(
    mut http_client: EmbassyHttpClient<'static, 'static, 3>,
    url: &'static str,
    period_ms: u64,
    name: &'static str,
) {
    loop {
        info!("[{}] Running HTTP client", name);
        let _ = http_client.get(url, 2).await;
        Timer::after(Duration::from_millis(period_ms)).await;
    }
}

type PhysFlash = FlashStorage;
type AsyncFlash = BlockingAsync<PhysFlash>;
type FlashLayer = DbFlash<AsyncFlash>;
type KvDatabase = ekv::Database<FlashLayer, CriticalSectionRawMutex>;
type DbMutex = Mutex<CriticalSectionRawMutex, KvDatabase>;
static DB: StaticCell<DbMutex> = StaticCell::new();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size: 72 * 1024);

    log_banner("Storage Init");
    let mut ota_flash = FlashStorage::new();
    info!("Flash size = {}", ota_flash.capacity());

    log_banner("Peripherals Init");
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    info!("CPU {:>3} MHz", config.cpu_clock().mhz());

    log_banner("OTA Init");
    {
        let mut pt_mem = [0u8; partitions::PARTITION_TABLE_MAX_LEN];
        let ota_result = run_with_ota(
            &mut ota_flash,
            &mut pt_mem,
            |ota| -> Result<(), ota::Error> {
                let current = ota.current_slot()?;
                if current != Slot::None {
                    ota.set_current_ota_state(Valid)?;
                }
                info!("current OTA image state {:?}", ota.current_ota_state()?);
                info!("current OTA {:?} → next {:?}", current, current.next());
                validate_current_ota_slot(ota)?;
                Ok(())
            },
        );
        try_log!(ota_result, "OTA init/validation failed");
    }

    log_banner("DB Init");
    let flash = FlashStorage::new();
    let async_flash = BlockingAsync::new(flash);
    let flash_layer = FlashLayer {
        start: CONFIG_PARTITION_START,
        flash: async_flash,
    };
    let kv = KvDatabase::new(flash_layer, ekv::Config::default());

    let kv_mutex: &'static DbMutex = DB.init(Mutex::new(kv));
    {
        let db = kv_mutex.lock().await;
        if db.mount().await.is_err() {
            info!("Formatting Persistent EKV Storage...");
            try_log!(db.format().await, "EKV format failed");
        }
    }

    {
        let db = kv_mutex.lock().await;
        let mut buf = [0u8; 32];
        if let Ok(n) = db
            .read_transaction()
            .await
            .read(b"wifi.ssid", &mut buf)
            .await
        {
            let s = &buf[..n];
            match core::str::from_utf8(s) {
                Ok(text) => info!("Wi‑Fi ssid: {}", text),
                Err(_) => info!("Wi‑Fi ssid (invalid UTF‑8): {:?}", s),
            }
        }
    }

    log_banner("NeoPixel init");
    let led_pin = peripherals.GPIO48;
    let freq = Rate::from_mhz(80);
    let rmt = match Rmt::new(peripherals.RMT, freq) {
        Ok(r) => r,
        Err(e) => {
            error!("RMT init failed: {:?}", e);
            loop {
                Timer::after(Duration::from_secs(60)).await;
            }
        }
    };

    static LED_CTRL: StaticCell<Signal<CriticalSectionRawMutex, bool>> = StaticCell::new();
    let led_ctrl_signal = &*LED_CTRL.init(Signal::new());

    log_banner("Timers Init");
    let timer_g0 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    let timer_g1 = TimerGroup::new(peripherals.TIMG1);
    let timer0: AnyTimer = timer_g1.timer0.into();
    let timer1: AnyTimer = timer_g1.timer1.into();
    esp_hal_embassy::init([timer0, timer1]);

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);

    log_banner("Led Worker Init");
    let worker_guard =
        match cpu_control.start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());

            executor.run(|spawner| {
                match spawner.spawn(control_led(led_pin, rmt, led_ctrl_signal)) {
                    Ok(_) => info!("Worker task spawned successfully"),
                    Err(e) => error!("Failed to spawn worker task: {:?}", e),
                }
            });
        }) {
            Ok(guard) => {
                info!("Core started successfully");
                guard
            }
            Err(e) => {
                error!("Failed to start core: {:?}", e);
                return;
            }
        };

    static mut WORKER_GUARD: Option<AppCoreGuard> = None; // Store the AppCoreGuard to keep the core alive
    unsafe {
        WORKER_GUARD = Some(worker_guard);
    }

    log_banner("Led Control Init");
    try_log!(
        spawner.spawn(enable_disable_led(led_ctrl_signal)),
        "spawn(enable_disable_led)"
    );

    log_banner("Wifi Init");
    let (ssid, password, mode) = match get_wifi_credentials(kv_mutex).await {
        Ok(creds) => {
            info!("Using stored Wi-Fi credentials");
            info!("mDNS name {}.local", creds.hostname);
            (creds.ssid, creds.password, WifiMode::Sta)
        }
        Err(_) => match get_default_wifi_credentials() {
            Ok(default_creds)
                if !default_creds.ssid.is_empty() && default_creds.ssid != "MyDefaultSSID" =>
            {
                info!("Using compile-time Wi-Fi credentials");
                info!("mDNS name {}.local", default_creds.hostname);
                (default_creds.ssid, default_creds.password, WifiMode::Sta)
            }
            _ => {
                info!("No valid credentials, starting in AP mode");
                (String::new(), String::new(), WifiMode::Ap)
            }
        },
    };

    let stack = match init_wifi(
        spawner,
        timer_g0,
        rng,
        peripherals.WIFI,
        peripherals.RADIO_CLK,
        ssid,
        password,
        mode,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("Wi‑Fi init failed: {:?}", e);
            loop {
                Timer::after(Duration::from_secs(60)).await;
            }
        }
    };

    WIFI_INITIALIZED.store(true, Ordering::Release);
    log_banner("System Init finished");

    let client_state = CLIENT_STATE.init(TcpClientState::new());
    let tcp_client = TCP_CLIENT.init(TcpClient::new(*stack, client_state));
    let http_client = EmbassyHttpClient::new(stack, tcp_client);

    log_banner("HTTP Clients Init finished");

    log_banner("Starting http worker");
    try_log!(
        spawner.spawn(http_wk(
            http_client,
            "http://mobile-j.de",
            120_000,
            "client1"
        )),
        "spawn(http_wk)"
    );

    log_banner("Starting web server");
    let sse_message_watch = web_server::init_sse_message_watch();
    let sse_message_sender = sse_message_watch.sender();
    let app_props = AppProps::new(kv_mutex);
    let app = make_static!(AppRouter<AppProps>, app_props.build_app());
    let config = make_static!(
        picoserve::Config<Duration>,
        picoserve::Config::new(picoserve::Timeouts {
            persistent_start_read_request: Some(Duration::from_secs(5)),
            start_read_request: Some(Duration::from_secs(5)),
            read_request: Some(Duration::from_secs(1)),
            write: Some(Duration::from_secs(1)),
        })
        .keep_connection_alive()
    );
    for id in 0..web_server::WEB_TASK_POOL_SIZE {
        spawner.must_spawn(web_task(id, *stack, app, config));
    }

    sse_message_sender.clear();
    if let Ok(msg) = "Hello SSE!".parse() {
        sse_message_sender.send(msg);
    } else {
        error!("Failed to parse initial SSE message");
    }

    log_banner("Mqtt Init");
    let (mqtt_broker_uri, mqtt_client_id, mqtt_username, mqtt_password) =
        match get_mqtt_credentials(kv_mutex).await {
            Ok(mqtt) => {
                info!("Using stored MQTT credentials");
                (
                    mqtt.broker_uri,
                    mqtt.client_id,
                    mqtt.username,
                    mqtt.password,
                )
            }
            Err(_) => match get_default_mqtt_credentials() {
                Ok(default_mqtt_creds)
                    if !default_mqtt_creds.broker_uri.is_empty()
                        && default_mqtt_creds.broker_uri != "tcp://localhost:1883" =>
                {
                    info!("Using compile-time MQTT credentials");
                    (
                        default_mqtt_creds.broker_uri,
                        default_mqtt_creds.client_id,
                        default_mqtt_creds.username,
                        default_mqtt_creds.password,
                    )
                }
                _ => {
                    info!("No valid MQTT credentials, skipping");
                    (String::new(), String::new(), String::new(), String::new())
                }
            },
        };
    info!("MQTT Broker URI {}", mqtt_broker_uri);
    info!("MQTT Client ID {}", mqtt_client_id);
    info!("MQTT Client USERNAME {}", mqtt_username);
    info!("MQTT Client PASSWORD {}", mqtt_password);

    log_banner("All Init finished");
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
