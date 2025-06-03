use alloc::boxed::Box;
use embassy_net::dns::Error as DnsError;
use embassy_net::tcp::ConnectError as TcpConnectError;
use embassy_net::tcp::Error as TcpError;
use embassy_net::{dns::DnsSocket, tcp::client::TcpClient, Stack};
use embassy_time::{with_timeout, Duration, TimeoutError};
use log::{info, warn};
use heapless::Vec;
use reqwless::client::HttpClient;
use reqwless::Error as ReqlessError;

const RESPONSE_SIZE: usize = 1024;

pub struct EmbassyHttpClient<
    'a,
    'b,
    const N: usize,
    const TX_SZ: usize = 1024,
    const RX_SZ: usize = 1024,
> {
    http_client: HttpClient<'a, TcpClient<'b, N, TX_SZ, RX_SZ>, DnsSocket<'b>>,
}

impl<'a, 'b, const N: usize, const TX_SZ: usize, const RX_SZ: usize>
    EmbassyHttpClient<'a, 'b, N, TX_SZ, RX_SZ>
{
    pub fn new(stack: &'b Stack<'static>, tcp_client: &'a TcpClient<'b, N, TX_SZ, RX_SZ>) -> Self {
        let dns = DnsSocket::new(*stack);
        let leaked_dns: &'static DnsSocket<'static> = Box::leak(Box::new(dns)); // Allocate on heap
        let http_client = HttpClient::new(tcp_client, leaked_dns);
        Self { http_client }
    }

    /// Send a GET request to the specified URL with timeout
    pub async fn get(&mut self, url: &str, timeout: u64) -> Result<Vec<u8, RESPONSE_SIZE>, Error> {
        let mut buffer = [0; RESPONSE_SIZE];

        let request_future = self
            .http_client
            .request(reqwless::request::Method::GET, url);
        let request_result = with_timeout(Duration::from_secs(timeout), request_future).await;

        info!("Sending HTTP request");
        let mut request = match request_result {
            Ok(Ok(req)) => req,
            Ok(Err(e)) => {
                info!("Error creating request: {:?}", e);
                return Err(Error::from(e));
            }
            Err(_) => {
                warn!("Timeout out creating HTTP request!");
                return Err(Error::from(TimeoutError));
            }
        };

        let response = request.send(&mut buffer).await?;
        info!("HTTP status: {:?}", response.status);

        let buffer = response.body().read_to_end().await?;
        info!("Read {} bytes", buffer.len());
        let output =
            Vec::<u8, RESPONSE_SIZE>::from_slice(buffer).map_err(|()| Error::ResponseTooLarge)?;

        Ok(output)
    }
}

/// An error within an HTTP request
#[derive(Debug)]
pub enum Error {
    /// Response was too large
    ResponseTooLarge,

    /// Error within TCP streams
    Tcp(TcpError),

    /// Error within TCP connection
    TcpConnect(TcpConnectError),

    /// Error within DNS system
    Dns(DnsError),

    /// Error in HTTP client
    Reqless(ReqlessError),

    Timeout(TimeoutError),
}

impl From<TcpError> for Error {
    fn from(error: TcpError) -> Self {
        Self::Tcp(error)
    }
}

impl From<TcpConnectError> for Error {
    fn from(error: TcpConnectError) -> Self {
        Self::TcpConnect(error)
    }
}

impl From<DnsError> for Error {
    fn from(error: DnsError) -> Self {
        Self::Dns(error)
    }
}

impl From<ReqlessError> for Error {
    fn from(error: ReqlessError) -> Self {
        Self::Reqless(error)
    }
}

impl From<TimeoutError> for Error {
    fn from(error: TimeoutError) -> Self {
        Self::Timeout(error)
    }
}
