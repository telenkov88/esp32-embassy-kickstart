use embassy_executor::task;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::gpio::Output;
use esp_println::println;
use esp_hal::system::Cpu;


/// Waits for a message that contains a duration, then flashes a LED for that
/// duration of time.
#[task]
pub async fn control_led(
    mut led: Output<'static>,
    control: &'static Signal<CriticalSectionRawMutex, bool>,
) {
    println!("Starting control_led() on core {}", Cpu::current() as usize);
    loop {
        if control.wait().await {
            println!("LED on from CPU{}", Cpu::current() as usize);
            led.set_low();
        } else {
            println!("LED off");
            led.set_high();
        }
    }
}
