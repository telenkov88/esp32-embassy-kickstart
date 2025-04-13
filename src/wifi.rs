use embassy_executor::{Spawner, task};
use embassy_net::{Runner, StackResources, Stack};
use embassy_time::{Duration, Timer};

use esp_println::println;
use esp_wifi::{
    init,
    wifi::{ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState, WifiError},
    EspWifiController,
    InitializationError
};

use esp_hal::peripherals::RADIO_CLK;
use esp_hal::peripherals::TIMG0;
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use static_cell::StaticCell;

pub static STACK_RESOURCES: StaticCell<StackResources<20>> = StaticCell::new();
pub static WIFI_STACK: StaticCell<Stack> = StaticCell::new();

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}


pub async fn connect(spawner: Spawner,
                          timer_g0: TimerGroup<TIMG0>,
                          mut rng: Rng,
                          wifi: WIFI,
                          radio_clock_control: RADIO_CLK,
                          ssid: &'static str, password: &'static str) -> Result<&'static Stack<'static>, Error> {
    esp_println::logger::init_logger_from_env();

    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        init(timer_g0.timer0, rng.clone(), radio_clock_control)?
    );

    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, wifi)?;

    let wifi_interface = interfaces.sta;

    let config = embassy_net::Config::dhcpv4(Default::default());

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let resources = STACK_RESOURCES.init(StackResources::<20>::new());
    // Init network stack
    let (temp_stack, runner) = embassy_net::new(wifi_interface, config, resources, seed);
    // Initialize the global WIFI_STACK.
    let stack = WIFI_STACK.init(temp_stack);
    
    spawner.spawn(connection(controller, ssid, password)).ok();
    spawner.spawn(net_task(runner)).ok();

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = temp_stack.config_v4() {
            println!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    println!("Leave connection task");
    Ok(stack)
}

#[task]
async fn connection(mut controller: WifiController<'static>, ssid: &'static str, password: &'static str)  {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::StaConnected => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: ssid.try_into().unwrap(),
                password: password.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }
        println!("About to connect SSID {}", ssid);

        match controller.connect_async().await {
            Ok(_) => println!("Wifi connected!"),
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}


#[derive(Debug)]
pub enum Error {
    /// Error during Wi-Fi initialization
    WifiInitialization(#[expect(unused, reason = "Never read directly")] InitializationError),

    /// Error during Wi-Fi operation
    Wifi(#[expect(unused, reason = "Never read directly")] WifiError),
}

impl From<InitializationError> for Error {
    fn from(error: InitializationError) -> Self {
        Self::WifiInitialization(error)
    }
}

impl From<WifiError> for Error {
    fn from(error: WifiError) -> Self {
        Self::Wifi(error)
    }
}