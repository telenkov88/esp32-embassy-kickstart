use core::net::Ipv4Addr;
use core::str::FromStr;
use core::sync::atomic::Ordering;
use embassy_executor::{Spawner, task};
use embassy_net::{Runner, StackResources, Stack, StaticConfigV4, Ipv4Cidr};
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
use esp_wifi::wifi::AccessPointConfiguration;
use heapless::String;
use static_cell::StaticCell;
use crate::WIFI_MODE_CLIENT;

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

pub enum WifiMode {
    Sta,
    Ap,
}


#[task]
async fn run_dhcp(stack: Stack<'static>, gw_ip_addr: &'static str) {
    use core::net::{Ipv4Addr, SocketAddrV4};

    use edge_dhcp::{
        io::{self, DEFAULT_SERVER_PORT},
        server::{Server, ServerOptions},
    };
    use edge_nal::UdpBind;
    use edge_nal_embassy::{Udp, UdpBuffers};

    let ip = Ipv4Addr::from_str(gw_ip_addr).expect("dhcp task failed to parse gw ip");

    let mut buf = [0u8; 1500];

    let mut gw_buf = [Ipv4Addr::UNSPECIFIED];

    let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
    let unbound_socket = Udp::new(stack, &buffers);
    let mut bound_socket = unbound_socket
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
        .unwrap();

    loop {
        _ = io::server::run(
            &mut Server::<_, 64>::new_with_et(ip),
            &ServerOptions::new(ip, Some(&mut gw_buf)),
            &mut bound_socket,
            &mut buf,
        )
            .await
            .inspect_err(|e| println!("DHCP server error: {e:?}"));
        Timer::after(Duration::from_millis(500)).await;
    }
}


pub async fn init_wifi(spawner: Spawner,
                       timer_g0: TimerGroup<TIMG0>,
                       mut rng: Rng,
                       wifi: WIFI,
                       radio_clock_control: RADIO_CLK,
                       ssid: String<32>,
                       password: String<64>,
                       mode: WifiMode,) -> Result<&'static Stack<'static>, Error> {
    esp_println::logger::init_logger_from_env();

    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        init(timer_g0.timer0, rng.clone(), radio_clock_control)?
    );

    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, wifi)?;

    let (device, config) = match mode {
        WifiMode::Sta => (
            interfaces.sta,
            embassy_net::Config::dhcpv4(Default::default()),
        ),
        WifiMode::Ap => {
            let gw_ip_addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
            (
                interfaces.ap,
                embassy_net::Config::ipv4_static(StaticConfigV4 {
                    address: Ipv4Cidr::new(gw_ip_addr, 24),
                    gateway: Some(gw_ip_addr),
                    dns_servers: Default::default(),
                }),
            )
        }
    };

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    
    let resources = STACK_RESOURCES.init(StackResources::<20>::new());
    // Init network stack
    let (temp_stack, runner) = embassy_net::new(device, config, resources, seed);
    // Initialize the global WIFI_STACK.
    let stack = WIFI_STACK.init(temp_stack);

    match mode {
        WifiMode::Sta => {
            println!("Connect Sta Mode");
            WIFI_MODE_CLIENT.store(true, Ordering::Release);
            spawner.spawn(wifi_connection(controller, mode, Some(ssid), Some(password))).ok();
        }
        WifiMode::Ap => {
            println!("Connect AP Mode");
            WIFI_MODE_CLIENT.store(false, Ordering::Release);
            spawner.spawn(wifi_connection(controller, mode, None, None)).ok();
            spawner.spawn(run_dhcp(*stack, "192.168.1.1")).ok();
        }
    }


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
async fn wifi_connection(
    mut controller: WifiController<'static>,
    mode: WifiMode,
    ssid: Option<String<32>>,
    password: Option<String<64>>,
) {
    println!("Start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());

    if let WifiMode::Sta = &mode {
        println!("SSID: {:?} Password: {:?}", ssid, password);
    }

    loop {
        let current_mode = &mode; // Take a reference to avoid moving

        match (esp_wifi::wifi::wifi_state(), current_mode) {
            (WifiState::StaConnected, WifiMode::Sta) => {
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await;
            }
            (WifiState::ApStarted, WifiMode::Ap) => {
                controller.wait_for_event(WifiEvent::ApStop).await;
                Timer::after(Duration::from_millis(5000)).await;
            }
            _ => {}
        }

        if !matches!(controller.is_started(), Ok(true)) {
            let config = match current_mode {
                WifiMode::Sta => {
                    Configuration::Client(ClientConfiguration {
                        ssid: ssid.clone().unwrap(),
                        password: password.clone().unwrap(),
                        ..Default::default()
                    })
                }
                WifiMode::Ap => {
                    Configuration::AccessPoint(AccessPointConfiguration {
                        ssid: "esp-wifi".try_into().unwrap(),
                        ..Default::default()
                    })
                }
            };

            controller.set_configuration(&config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }

        if let WifiMode::Sta = current_mode {
            println!("About to connect SSID {:?}", ssid);
            match controller.connect_async().await {
                Ok(_) => println!("Wifi connected!"),
                Err(e) => {
                    println!("Failed to connect to wifi: {e:?}");
                    Timer::after(Duration::from_millis(5000)).await;
                }
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