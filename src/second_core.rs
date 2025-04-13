use embassy_executor::task;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{rmt::Rmt};
use esp_hal::gpio::{GpioPin};
use esp_println::println;
use esp_hal::system::Cpu;
use esp_hal_smartled::smartLedBuffer;
use crate::neopixel::NeoPixel;


const GPIONUM: u8 = 48;

#[task]
pub async fn control_led(
    led: GpioPin<GPIONUM>,
    rmt: Rmt<'static, esp_hal::Blocking>,
    control: &'static Signal<CriticalSectionRawMutex, bool>,
) {
    println!("Starting control_led() on core {}", Cpu::current() as usize);
    let rmt_buffer = smartLedBuffer!(1);
    let channel = rmt.channel0;
    let mut smart_led = NeoPixel::new(channel, led, rmt_buffer);
    
    smart_led.set_brightness(0).unwrap();
    smart_led.set_hue(0).unwrap();
    
    loop {
        if control.wait().await {
            println!("LED on from CPU{}", Cpu::current() as usize);
            smart_led.set_rgb(0,255,0, 20).unwrap()
        } else {
            println!("LED off");
            smart_led.set_rgb(0,0,255, 20).unwrap()
        }
    }
}
