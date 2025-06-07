use crate::neopixel::NeoPixel;
use crate::{FIRMWARE_UPGRADE_IN_PROGRESS, WIFI_INITIALIZED, WIFI_MODE_CLIENT, try_log};
use core::sync::atomic::Ordering;
use embassy_executor::task;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::peripherals::GPIO48;
use esp_hal::rmt::Rmt;
use esp_hal::system::Cpu;
use esp_hal_smartled::smart_led_buffer;
use log::{error, info};

#[task]
pub async fn control_led(
    led: GPIO48<'static>,
    rmt: Rmt<'static, esp_hal::Blocking>,
    control: &'static Signal<CriticalSectionRawMutex, bool>,
) {
    info!("Starting control_led() on core {}", Cpu::current() as usize);

    let rmt_buffer = smart_led_buffer!(1);
    let channel = rmt.channel0;
    let mut smart_led = NeoPixel::new(channel, led, rmt_buffer);

    // Initial LED state -------------------------------------------------------
    try_log!(smart_led.set_brightness(0), "set_brightness(0)");
    try_log!(smart_led.set_hue(0), "set_hue(0)");
    try_log!(smart_led.set_rgb(0, 0, 0, 0), "set_rgb init");

    // Working buffers ---------------------------------------------------------
    let mut brightness: u8;
    let mut r: u8;
    let mut g: u8;
    let mut b: u8;

    // Main loop ---------------------------------------------------------------
    loop {
        // chosen by the button / external control
        brightness = if control.wait().await { 2 } else { 1 };

        // system-state → colour mapping
        match (
            FIRMWARE_UPGRADE_IN_PROGRESS.load(Ordering::Acquire),
            WIFI_INITIALIZED.load(Ordering::Acquire),
            WIFI_MODE_CLIENT.load(Ordering::Acquire),
        ) {
            (true, _, _) => {
                // dark-orange : firmware upgrade
                (r, g, b) = (255, 140, 0);
            }
            (false, true, true) => {
                // green : Wi-Fi client connected
                (r, g, b) = (0, 255, 0);
            }
            (false, true, false) => {
                // blue : AP mode
                (r, g, b) = (0, 0, 255);
            }
            _ => {
                // red : Wi-Fi offline
                (r, g, b) = (255, 0, 0);
            }
        }

        try_log!(smart_led.set_rgb(r, g, b, brightness), "set_rgb loop");
    }
}
