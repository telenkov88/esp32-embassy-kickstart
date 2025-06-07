use core::net::Ipv4Addr;
use core::str::FromStr;
use core::sync::atomic::Ordering;
use embassy_executor::{Spawner, task};
use embassy_net::{Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_wifi::{
    EspWifiController, InitializationError, init,
    wifi::{
        ClientConfiguration, Configuration, WifiController, WifiDevice, WifiError, WifiEvent,
        WifiState,
    },
};
use log::{error, info};

use crate::WIFI_MODE_CLIENT;
use esp_hal::peripherals::RADIO_CLK;
use esp_hal::peripherals::TIMG0;
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::AccessPointConfiguration;
use heapless::String;
use static_cell::StaticCell;

pub static STACK_RESOURCES: StaticCell<StackResources<10>> = StaticCell::new();
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

    let ip = match Ipv4Addr::from_str(gw_ip_addr) {
        Ok(ip) => ip,
        Err(e) => {
            error!("DHCP server: invalid gateway IP address '{gw_ip_addr}': {e}");
            return;
        }
    };

    let mut buf = [0u8; 1500];

    let mut gw_buf = [Ipv4Addr::UNSPECIFIED];

    let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
    let unbound_socket = Udp::new(stack, &buffers);
    let mut bound_socket = match unbound_socket
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
    {
        Ok(sock) => sock,
        Err(e) => {
            error!("DHCP server: failed to bind socket: {e:?}");
            return;
        }
    };

    loop {
        if let Err(e) = io::server::run(
            &mut Server::<_, 64>::new_with_et(ip),
            &ServerOptions::new(ip, Some(&mut gw_buf)),
            &mut bound_socket,
            &mut buf,
        )
        .await
        {
            error!("DHCP server error: {e:?}");
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn init_wifi(
    spawner: Spawner,
    timer_g0: TimerGroup<'static, TIMG0<'static>>,
    mut rng: Rng,
    wifi: WIFI<'static>,
    radio_clock_control: RADIO_CLK<'static>,
    ssid: String<32>,
    password: String<64>,
    mode: WifiMode,
) -> Result<&'static Stack<'static>, Error> {
    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        init(timer_g0.timer0, rng, radio_clock_control)?
    );

    let (controller, interfaces) = esp_wifi::wifi::new(esp_wifi_ctrl, wifi)?;

    let (device, config) = match mode {
        WifiMode::Sta => (
            interfaces.sta,
            embassy_net::Config::dhcpv4(Default::default()),
        ),
        WifiMode::Ap => {
            let gw_ip_addr = Ipv4Addr::new(192, 168, 1, 1);
            (
                interfaces.ap,
                embassy_net::Config::ipv4_static(StaticConfigV4 {
                    address: Ipv4Cidr::new(gw_ip_addr, 28),
                    gateway: Some(gw_ip_addr),
                    dns_servers: Default::default(),
                }),
            )
        }
    };

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let resources = STACK_RESOURCES.init(StackResources::<10>::new());
    let (temp_stack, runner) = embassy_net::new(device, config, resources, seed);
    let stack = WIFI_STACK.init(temp_stack);

    match mode {
        WifiMode::Sta => {
            info!("Connect Sta Mode");
            WIFI_MODE_CLIENT.store(true, Ordering::Release);
            if let Err(e) = spawner.spawn(wifi_connection(
                controller,
                mode,
                Some(ssid),
                Some(password),
            )) {
                error!("Failed to spawn wifi_connection: {e:?}");
            }
        }
        WifiMode::Ap => {
            info!("Connect AP Mode");
            WIFI_MODE_CLIENT.store(false, Ordering::Release);
            if let Err(e) = spawner.spawn(wifi_connection(controller, mode, None, None)) {
                error!("Failed to spawn wifi_connection: {e:?}");
            }
            if let Err(e) = spawner.spawn(run_dhcp(*stack, "192.168.1.1")) {
                error!("Failed to spawn DHCP task: {e:?}");
            }
        }
    }

    if let Err(e) = spawner.spawn(net_task(runner)) {
        error!("Failed to spawn net task: {e:?}");
    }

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Waiting to get IP address...");
    loop {
        if let Some(config) = temp_stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    info!("Leave connection task");
    Ok(stack)
}

#[task]
async fn wifi_connection(
    mut controller: WifiController<'static>,
    mode: WifiMode,
    ssid: Option<String<32>>,
    password: Option<String<64>>,
) {
    info!("Start connection task");
    info!("Device capabilities: {:?}", controller.capabilities());

    if let WifiMode::Sta = &mode {
        info!("SSID: {:?} Password: {:?}", ssid, password);
    }

    loop {
        let current_mode = &mode;

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

        match controller.is_started() {
            Ok(true) => {} // already started
            Ok(false) => {
                let config = match current_mode {
                    WifiMode::Sta => {
                        let ssid_val = match ssid.clone() {
                            Some(v) => v,
                            None => {
                                error!("STA mode requested but SSID not provided");
                                Timer::after(Duration::from_millis(5000)).await;
                                continue;
                            }
                        };
                        let password_val = match password.clone() {
                            Some(v) => v,
                            None => {
                                error!("STA mode requested but password not provided");
                                Timer::after(Duration::from_millis(5000)).await;
                                continue;
                            }
                        };

                        let ssid_str = match core::str::from_utf8(ssid_val.as_bytes()) {
                            Ok(s) => s,
                            Err(e) => {
                                error!("Invalid UTF-8 in SSID: {}", e);
                                Timer::after(Duration::from_millis(5000)).await;
                                continue;
                            }
                        };

                        let password_str = match core::str::from_utf8(password_val.as_bytes()) {
                            Ok(s) => s,
                            Err(e) => {
                                error!("Invalid UTF-8 in password: {}", e);
                                Timer::after(Duration::from_millis(5000)).await;
                                continue;
                            }
                        };

                        Configuration::Client(ClientConfiguration {
                            ssid: ssid_str.into(),
                            password: password_str.into(),
                            ..Default::default()
                        })
                    }
                    WifiMode::Ap => {
                        let ap_ssid: String<32> = match "esp-wifi".try_into() {
                            Ok(s) => s,
                            Err(_) => {
                                error!("Failed to create AP SSID string");
                                Timer::after(Duration::from_millis(5000)).await;
                                continue;
                            }
                        };

                        let ssid_str = match core::str::from_utf8(ap_ssid.as_bytes()) {
                            Ok(s) => s,
                            Err(e) => {
                                error!("Invalid UTF-8 in AP SSID: {}", e);
                                Timer::after(Duration::from_millis(5000)).await;
                                continue;
                            }
                        };

                        Configuration::AccessPoint(AccessPointConfiguration {
                            ssid: ssid_str.into(),
                            ..Default::default()
                        })
                    }
                };

                if let Err(e) = controller.set_configuration(&config) {
                    error!("set_configuration error: {e:?}");
                    Timer::after(Duration::from_millis(5000)).await;
                    continue;
                }
                info!("Starting wifi");
                if let Err(e) = controller.start_async().await {
                    error!("start_async error: {e:?}");
                    Timer::after(Duration::from_millis(5000)).await;
                    continue;
                }
                info!("Wifi started!");
            }
            Err(e) => {
                error!("is_started() failed: {e:?}");
                Timer::after(Duration::from_millis(5000)).await;
                continue;
            }
        }

        if let WifiMode::Sta = current_mode {
            info!("About to connect SSID {:?}", ssid);
            match controller.connect_async().await {
                Ok(_) => info!("Wifi connected!"),
                Err(e) => {
                    error!("Failed to connect to wifi: {e:?}");
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
