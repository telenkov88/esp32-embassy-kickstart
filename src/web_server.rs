use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::{Watch};
use embassy_time::{Duration, Timer};
use esp_hal::xtensa_lx::_export::critical_section;
use esp_println::println;
use heapless::String;
use picoserve::{AppBuilder, AppRouter};
use picoserve::io::embedded_io_async;
use picoserve::response::ws;
use picoserve::response::sse;
use picoserve::routing::{get, get_service};
use static_cell::StaticCell;

pub const WEB_TASK_POOL_SIZE: usize = 12; // Panic if embassy_executor arena too small

// SSE buffer
pub type MessageWatch = Watch<CriticalSectionRawMutex, String<128>, 1>;
// Static storage for the watch channel
static SSE_MESSAGE_WATCH: StaticCell<MessageWatch> = StaticCell::new();

// Global reference to our watch
static mut WATCH_REF: Option<&'static MessageWatch> = None;

// Initialize the message watch once at startup
pub fn init_sse_message_watch() -> &'static MessageWatch {
    let watch = SSE_MESSAGE_WATCH.init(Watch::new());

    // Store a static reference to the watch for later use
    critical_section::with(|_| {
        unsafe { WATCH_REF = Some(watch); }
    });

    watch
}


pub fn get_sse_watch_ref() -> &'static MessageWatch {
    critical_section::with(|_| {
        unsafe { WATCH_REF.expect("Message watch not initialized") }
    })
}

pub struct SseEvents {}

impl SseEvents {
    pub fn new() -> Self {
        Self {}
    }
}

pub fn create_sse_events() -> SseEvents {
    SseEvents::new()
}


pub struct AppProps;

impl AppBuilder for AppProps {
    type PathRouter = impl picoserve::routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {

        picoserve::Router::new()
            .route(
                "/",
                get_service(picoserve::response::File::html(include_str!("http/index.html"))),
            )
            .route(
                "/index.css",
                get_service(picoserve::response::File::css(include_str!("http/index.css"))),
            )
            .route(
                "/favicon.ico",
                get_service(picoserve::response::File::with_content_type("image/vnd.microsoft.icon", include_bytes!("http/favicon.ico"))),
            )
            .route(
                "/index.js",
                get_service(picoserve::response::File::javascript(include_str!(
                    "http/index.js"
                ))),
            )
            .route(
                "/ws",
                get(|upgrade: picoserve::response::WebSocketUpgrade| {
                    upgrade.on_upgrade(WebsocketEcho).with_protocol("echo")
                }),
            )
            .route(
                "/events",
                get(|| picoserve::response::EventStream(create_sse_events())),
            )
    }
}

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    id: usize,
    stack: embassy_net::Stack<'static>,
    app: &'static AppRouter<AppProps>,
    config: &'static picoserve::Config<Duration>,
) -> ! {
    let port = 80;
    let mut tcp_rx_buffer = [0; 1024];
    let mut tcp_tx_buffer = [0; 1024];
    let mut http_buffer = [0; 2048];

    picoserve::listen_and_serve(
        id,
        app,
        config,
        stack,
        port,
        &mut tcp_rx_buffer,
        &mut tcp_tx_buffer,
        &mut http_buffer,
    )
        .await
}

impl sse::EventSource for SseEvents {
    async fn write_events<W: picoserve::io::Write>(
        self,
        mut writer: sse::EventWriter<W>,
    ) -> Result<(), W::Error> {
        let watch = get_sse_watch_ref();
        let mut receiver = match watch.receiver() {
            Some(r) => r,
            None => {
                // Log the error and perhaps return early.
                println!("Error: The watch channel is closed. Cannot subscribe for events.");
                return Ok(()); // or handle the failure as appropriate
            }
        };
        writer.write_event("message_changed", "").await?;


        loop {
            // Wait for message changes or timeout after 10 seconds
            match embassy_futures::select::select(
                receiver.changed(),
                Timer::after(Duration::from_secs(10))
            ).await {
                embassy_futures::select::Either::First(result) => {
                    if result == "" {
                        println!("SSE Result: {}. its Closed?", result);
                        // Channel closed
                        break Ok(());
                    } else {
                        println!("SSE Result: {}", result);
                    }
                    
                    let message: String<128> = receiver.get().await;
                    let message_slice: &str = message.as_str();

                    writer.write_event("message_changed", message_slice).await?;
                },
                embassy_futures::select::Either::Second(_) => {
                    // Timeout - send keepalive
                    writer.write_keepalive().await?;
                }
            }
        }
    }
}

//WSS
struct WebsocketEcho;

impl ws::WebSocketCallback for WebsocketEcho {
    async fn run<R: embedded_io_async::Read, W: embedded_io_async::Write<Error = R::Error>>(
        self,
        mut rx: ws::SocketRx<R>,
        mut tx: ws::SocketTx<W>,
    ) -> Result<(), W::Error> {
        let mut buffer = [0; 1024];

        let close_reason = loop {
            match rx.next_message(&mut buffer).await {
                Ok(ws::Message::Text(data)) => tx.send_text(data).await,
                Ok(ws::Message::Binary(data)) => tx.send_binary(data).await,
                Ok(ws::Message::Close(reason)) => {
                    println!("Websocket close reason: {reason:?}");
                    break None;
                }
                Ok(ws::Message::Ping(data)) => tx.send_pong(data).await,
                Ok(ws::Message::Pong(_)) => continue,
                Err(err) => {
                    println!("Websocket Error: {err:?}");

                    let code = match err {
                        ws::ReadMessageError::Io(err) => return Err(err),
                        ws::ReadMessageError::ReadFrameError(_)
                        | ws::ReadMessageError::MessageStartsWithContinuation
                        | ws::ReadMessageError::UnexpectedMessageStart => 1002,
                        ws::ReadMessageError::ReservedOpcode(_) => 1003,
                        ws::ReadMessageError::TextIsNotUtf8 => 1007,
                    };
                    break Some((code, "Websocket Error"));
                }
            }?;
        };

        tx.close(close_reason).await
    }
}
