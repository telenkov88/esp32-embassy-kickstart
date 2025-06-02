use crate::{DbMutex, KvDatabase, PASSWORD, SSID};
use core::fmt;
use ekv::{CommitError, ReadError, WriteError};
use esp_println::println;
use esp_storage::FlashStorageError;
use heapless::String;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct WifiCredentials {
    pub ssid: String<32>,
    pub password: String<64>,
    pub hostname: String<32>,
}

pub async fn get_wifi_credentials(
    db_mutex: &'static DbMutex,
) -> Result<WifiCredentials, WifiSettingsError> {
    let (_, ssid) = read_wifi_ssid(db_mutex).await?;
    let (_, password) = read_wifi_password(db_mutex).await?;
    let (_, hostname) = read_hostname(db_mutex).await?;

    if !ssid.is_empty() && !password.is_empty() {
        Ok(WifiCredentials {
            ssid,
            password,
            hostname,
        })
    } else {
        Err(WifiSettingsError::InvalidData)
    }
}

pub fn get_default_credentials() -> WifiCredentials {
    WifiCredentials {
        ssid: String::try_from(SSID).unwrap_or_default(),
        password: String::try_from(PASSWORD).unwrap_or_default(),
        hostname: String::try_from("esp-device").unwrap_or_default(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WifiSettings {
    pub(crate) hostname: String<32>,
    pub(crate) ssid: String<32>,
    pub(crate) psw: String<64>,
}

#[derive(Debug)]
pub enum WifiSettingsError {
    Storage(DbError),
    InvalidData,
}

impl fmt::Display for WifiSettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WifiSettingsError::Storage(e) => write!(f, "Storage error: {:?}", e),
            WifiSettingsError::InvalidData => write!(f, "Invalid data format"),
        }
    }
}

impl From<DbError> for WifiSettingsError {
    fn from(e: DbError) -> Self {
        WifiSettingsError::Storage(e)
    }
}

pub async fn update_wifi_settings(
    settings: &WifiSettings,
    db_mutex: &'static DbMutex,
) -> Result<bool, WifiSettingsError> {
    println!("Received new Wi-Fi settings:");
    println!("  • Hostname: {}", settings.hostname);
    println!("  • SSID:     {}", settings.ssid);
    println!("  • Password: {}", settings.psw);

    {
        let mut db = db_mutex.lock().await;
        write_db(&mut db, b"wifi.hostname", settings.hostname.as_bytes()).await?;
        write_db(&mut db, b"wifi.ssid", settings.ssid.as_bytes()).await?;
        write_db(&mut db, b"wifi.password", settings.psw.as_bytes()).await?;
    }

    let (len, ssid) = read_wifi_ssid(db_mutex).await?;
    if len == 0 {
        return Err(WifiSettingsError::InvalidData);
    }

    let verified = len == settings.ssid.len() && ssid.as_bytes() == settings.ssid.as_bytes();

    if verified {
        println!("✅  Wi-Fi settings saved and SSID verified.");
    } else {
        println!("⚠️  SSID verification FAILED!");
    }

    Ok(verified)
}

pub async fn read_setting<const N: usize>(
    db_mutex: &'static DbMutex,
    key: &[u8],
) -> Result<(usize, String<N>), DbError> {
    let mut buf = [0u8; N];
    let mut db = db_mutex.lock().await;
    let n = read_db(&mut db, key, &mut buf).await?;

    let mut setting = String::new();
    for &b in &buf[..n] {
        if setting.push(b as char).is_err() {
            println!("Truncation occurred for key {:?}", key);
            break;
        }
    }
    Ok((n, setting))
}

#[derive(Debug)]
pub enum DbError {
    Write(WriteError<FlashStorageError>),
    Commit(CommitError<FlashStorageError>),
    Read(ReadError<FlashStorageError>),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Write(e) => write!(f, "Write error: {:?}", e),
            DbError::Commit(e) => write!(f, "Commit error: {:?}", e),
            DbError::Read(e) => write!(f, "Read error: {:?}", e),
        }
    }
}

impl From<WriteError<FlashStorageError>> for DbError {
    fn from(e: WriteError<FlashStorageError>) -> Self {
        DbError::Write(e)
    }
}

impl From<CommitError<FlashStorageError>> for DbError {
    fn from(e: CommitError<FlashStorageError>) -> Self {
        DbError::Commit(e)
    }
}

impl From<ReadError<FlashStorageError>> for DbError {
    fn from(e: ReadError<FlashStorageError>) -> Self {
        DbError::Read(e)
    }
}

type DbResult<T> = Result<T, DbError>;

async fn write_db(db: &mut KvDatabase, key: &[u8], value: &[u8]) -> DbResult<()> {
    let mut tx = db.write_transaction().await;
    tx.write(key, value).await?;
    tx.commit().await?;
    Ok(())
}

async fn read_db(db: &mut KvDatabase, key: &[u8], buf: &mut [u8]) -> Result<usize, DbError> {
    let rtx = db.read_transaction().await;
    Ok(rtx.read(key, buf).await?)
}

pub async fn read_wifi_ssid(db_mutex: &'static DbMutex) -> Result<(usize, String<32>), DbError> {
    read_setting(db_mutex, b"wifi.ssid").await
}

pub async fn read_wifi_password(
    db_mutex: &'static DbMutex,
) -> Result<(usize, String<64>), DbError> {
    read_setting(db_mutex, b"wifi.password").await
}

pub async fn read_hostname(db_mutex: &'static DbMutex) -> Result<(usize, String<32>), DbError> {
    read_setting(db_mutex, b"wifi.hostname").await
}
