#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(adt_const_params)]
#![feature(impl_trait_in_assoc_type)]
extern crate alloc;

use crate::ota::{run_with_ota, validate_current_ota_slot};
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::{task, Spawner};
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
#[allow(unused_imports)]
use esp_backtrace as _;
use esp_hal::rng::Rng;
use esp_hal::{
    clock::CpuClock,
    system::{CpuControl, Stack},
    timer::{timg::TimerGroup, AnyTimer},
};
use esp_hal::{rmt::Rmt, time::Rate};
use esp_hal_embassy::Executor;
use log::info;
use static_cell::StaticCell;

mod neopixel;

mod http;
mod main_core;
mod second_core;
mod wifi;

mod shared;
mod web_server;

use crate::http::EmbassyHttpClient;
use main_core::enable_disable_led;
use second_core::control_led;
use wifi::init_wifi as connect_to_wifi;

use crate::web_server::AppProps;
use picoserve::{make_static, AppBuilder, AppRouter};
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
use log_utils::log_banner;

use crate::config::{get_default_credentials, get_wifi_credentials};
use crate::db::DbFlash;
use crate::wifi::WifiMode;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::mutex::Mutex;
use esp_hal::clock::Clock;
use heapless::String;

const CONFIG_PARTITION_START: usize = 0xA10000;

static mut APP_CORE_STACK: Stack<8192> = Stack::new();
static CLIENT_STATE: StaticCell<TcpClientState<3, 1024, 1024>> = StaticCell::new();
static TCP_CLIENT: StaticCell<TcpClient<'static, 3>> = StaticCell::new();

pub static WIFI_INITIALIZED: AtomicBool = AtomicBool::new(false);
pub static WIFI_MODE_CLIENT: AtomicBool = AtomicBool::new(false);
pub static TIME_SYNCED: AtomicBool = AtomicBool::new(false);
pub static FIRMWARE_UPGRADE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

const fn or_str(opt: Option<&'static str>, default: &'static str) -> &'static str {
    if let Some(val) = opt {
        val
    } else if let None = opt {
        default
    } else {
        unreachable!()
    }
}

const SSID: &str = or_str(option_env!("SSID"), "MyDefaultSSID");
const PASSWORD: &str = or_str(option_env!("PASSWORD"), "MyDefaultPassword");

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
        run_with_ota(&mut ota_flash, &mut pt_mem, |ota| {
            let current = ota.current_slot().unwrap();

            if current != Slot::None {
                ota.set_current_ota_state(Valid).unwrap();
            }
            info!("current OTA image state {:?}", ota.current_ota_state());
            info!("current OTA {:?} - next {:?}", current, current.next());

            validate_current_ota_slot(ota);
        })
        .unwrap();
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
            db.format().await.unwrap();
        }
    }

    {
        let db = kv_mutex.lock().await;
        let mut buf = [0u8; 32];
        let ssid = db
            .read_transaction()
            .await
            .read(b"wifi.ssid", &mut buf)
            .await
            .map(|n| &buf[..n])
            .ok();

        if let Some(s) = ssid {
            if let Ok(text) = core::str::from_utf8(s) {
                info!("Wi-Fi ssid: {}", text);
            } else {
                info!("Wi-Fi ssid (invalid UTF-8): {:?}", s);
            }
        }
    }

    log_banner("NeoPixel init");
    let led_pin = peripherals.GPIO48;
    let freq = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, freq).unwrap();
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
    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|spawner| {
                spawner
                    .spawn(control_led(led_pin, rmt, led_ctrl_signal))
                    .ok();
            });
        })
        .unwrap();
    log_banner("Led Control Init");
    spawner.spawn(enable_disable_led(led_ctrl_signal)).unwrap();

    log_banner("Wifi Init");
    let (ssid, password, mode) = match get_wifi_credentials(kv_mutex).await {
        Ok(creds) => {
            info!("Using stored Wi-Fi credentials");
            info!("mDNS name {}.local", creds.hostname);
            (creds.ssid, creds.password, WifiMode::Sta)
        }
        Err(_) => match get_default_credentials() {
            Ok(default_creds)
            if !default_creds.ssid.is_empty()
                && default_creds.ssid != "MyDefaultSSID" =>
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

    let stack = connect_to_wifi(
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
    .unwrap();

    WIFI_INITIALIZED.store(true, Ordering::Release);

    log_banner("System Init finished");

    let client_state = CLIENT_STATE.init(TcpClientState::new());
    let tcp_client = TCP_CLIENT.init(TcpClient::new(*stack, client_state));
    let http_client = EmbassyHttpClient::new(stack, tcp_client);

    log_banner("HTTP Clients Init finished");

    log_banner("Starting http worker");
    spawner
        .spawn(http_wk(
            http_client,
            "http://mobile-j.de",
            120000,
            "client1",
        ))
        .unwrap();

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
    sse_message_sender.send("Hello SSE!".parse().unwrap());

    log_banner("All Init finished");
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
