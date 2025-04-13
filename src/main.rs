//! Draft of ESP32S3 project.

//% CHIPS: esp32 esp32s3
//% FEATURES: embassy esp-hal/unstable

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(adt_const_params)]
extern crate alloc;

use core::ptr::addr_of_mut;

use embassy_executor::{task, Spawner};
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
#[allow(unused_imports)]
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock, system::{CpuControl, Stack}, timer::{timg::TimerGroup, AnyTimer}
};
use esp_hal::rng::Rng;
use esp_hal::{rmt::Rmt, time::Rate};
use esp_hal_embassy::Executor;
use esp_println::println;
use static_cell::StaticCell;

mod neopixel;

mod second_core;
mod main_core;
mod http;
mod wifi;
use second_core::control_led;
use main_core::enable_disable_led;
use wifi::connect as connect_to_wifi;
use crate::http::{EmbassyHttpClient};

static mut APP_CORE_STACK: Stack<8192> = Stack::new();
static CLIENT_STATE: StaticCell<TcpClientState<3, 1024, 1024>> = StaticCell::new();
static TCP_CLIENT: StaticCell<TcpClient<'static, 3>> = StaticCell::new();


static CLIENT_STATE2: StaticCell<TcpClientState<3, 1024, 1024>> = StaticCell::new();
static TCP_CLIENT2: StaticCell<TcpClient<'static, 3>> = StaticCell::new();

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
pub async fn http_wk(mut http_client: EmbassyHttpClient<'static,'static,3>, url: &'static str, period_ms: u64, name: &'static str,) {
    loop {
        println!("[{}] Running HTTP client", name);
        let _ = http_client.get(url, 2).await;
        Timer::after(Duration::from_millis(period_ms)).await;
    }
}


#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_alloc::heap_allocator!(size: 72 * 1024);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let timer_g0 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    let timer_g1 = TimerGroup::new(peripherals.TIMG1);
    let timer0: AnyTimer = timer_g1.timer0.into();
    let timer1: AnyTimer = timer_g1.timer1.into();
    esp_hal_embassy::init([timer0, timer1]);

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);

    static LED_CTRL: StaticCell<Signal<CriticalSectionRawMutex, bool>> = StaticCell::new();
    let led_ctrl_signal = &*LED_CTRL.init(Signal::new());
    
    let stack = connect_to_wifi(spawner, timer_g0, rng, peripherals.WIFI, peripherals.RADIO_CLK, SSID,  PASSWORD).await.unwrap();
    println!(">>>>>>>>>>> System Init finished <<<<<<<<<<<<<<<<<<<<<,");
    
    let client_state = CLIENT_STATE.init(TcpClientState::new());
    let tcp_client = TCP_CLIENT.init(TcpClient::new(*stack, client_state));
    let http_client1 = EmbassyHttpClient::new(stack, tcp_client);

    let client_state2 = CLIENT_STATE2.init(TcpClientState::new());
    let tcp_client2 = TCP_CLIENT2.init(TcpClient::new(*stack, client_state2));
    let http_client2 = EmbassyHttpClient::new(stack, tcp_client2);

    
    println!(">>>>>>>>>>> Init finished <<<<<<<<<<<<<<<<<<<<<<<<<<<<,");
    let led_pin = peripherals.GPIO48;
    let freq = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, freq).unwrap();

    println!(">>>> Starting http worker 1");
    spawner.spawn(http_wk(http_client1, "http://mobile-j.de", 120000, "client1")).unwrap();
    Timer::after(Duration::from_secs(5)).await;
    println!(">>>> Starting http worker 2");
    spawner.spawn(http_wk(http_client2, "http://mobile-j.de", 120000, "client2")).unwrap();

    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|spawner| {
                spawner.spawn(control_led(led_pin, rmt, led_ctrl_signal)).ok();
            });
        })
        .unwrap();

    enable_disable_led(led_ctrl_signal).await;
}
