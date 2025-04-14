use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_println::println;
use esp_hal::system::Cpu;
use embassy_time::{Duration, Ticker};

pub async fn enable_disable_led(led_ctrl_signal: &'static Signal<CriticalSectionRawMutex, bool>) -> ! {
    println!(
        "Starting enable_disable_led() on core {}",
        Cpu::current() as usize
    );
    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        //println!("Sending LED on");
        led_ctrl_signal.signal(true);
        ticker.next().await;

        //println!("Sending LED off");
        led_ctrl_signal.signal(false);
        ticker.next().await;
    }
}
