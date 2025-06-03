use embassy_executor::task;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Ticker};
use esp_hal::system::Cpu;
use log::{info};

#[task]
pub async fn enable_disable_led(
    led_ctrl_signal: &'static Signal<CriticalSectionRawMutex, bool>,
) -> ! {
    info!(
        "Starting enable_disable_led() on core {}",
        Cpu::current() as usize
    );
    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        led_ctrl_signal.signal(true);
        ticker.next().await;

        led_ctrl_signal.signal(false);
        ticker.next().await;
    }
}
