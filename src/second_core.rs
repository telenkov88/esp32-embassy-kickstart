use crate::neopixel::NeoPixel;
use crate::{FIRMWARE_UPGRADE_IN_PROGRESS, WIFI_INITIALIZED, WIFI_MODE_CLIENT};
use core::sync::atomic::Ordering;
use embassy_executor::task;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::gpio::GpioPin;
use esp_hal::rmt::Rmt;
use esp_hal::system::Cpu;
use esp_hal_smartled::smartLedBuffer;
use esp_println::println;

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
    let mut brightness: u8 = 0;
    let mut r: u8 = 0;
    let mut g: u8 = 0;
    let mut b: u8 = 0;
    smart_led.set_hue(0).unwrap();
    smart_led.set_rgb(r, g, b, brightness).unwrap();
    loop {
        if control.wait().await {
            brightness = 2;
        } else {
            brightness = 1;
        }
        if FIRMWARE_UPGRADE_IN_PROGRESS.load(Ordering::Acquire) {
            r = 255;
            g = 140;
            b = 0; // Firmware upgrade in progress. dark orange
        } else if WIFI_INITIALIZED.load(Ordering::Acquire)
            && WIFI_MODE_CLIENT.load(Ordering::Acquire)
        {
            r = 0;
            g = 255;
            b = 0; // Wi-fi online, Client mod. Green
        } else if WIFI_INITIALIZED.load(Ordering::Acquire)
            && !WIFI_MODE_CLIENT.load(Ordering::Acquire)
        {
            r = 0;
            g = 0;
            b = 255; // Wi-fi online, AP mod.     Blue
        } else {
            r = 255;
            g = 0;
            b = 0; // Wi-fi offline.            Red
        }

        smart_led.set_rgb(r, g, b, brightness).unwrap()
    }
}
