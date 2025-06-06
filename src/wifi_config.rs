use crate::config::{DbError, read_setting, write_db};
use crate::shared::or_str;
use crate::{DbMutex, KvDatabase};
use core::fmt;
use ekv::{CommitError, ReadError, WriteError};
use esp_storage::FlashStorageError;
use heapless::String;
use log::{error, info};
use serde::{Deserialize, Serialize};

pub const SSID: &str = or_str(option_env!("SSID"), "MyDefaultSSID");
pub const PASSWORD: &str = or_str(option_env!("PASSWORD"), "MyDefaultPassword");

const WIFI_SSID_KEY: &[u8] = b"wifi.ssid";
const WIFI_PASSWORD_KEY: &[u8] = b"wifi.password";
const WIFI_HOSTNAME_KEY: &[u8] = b"wifi.hostname";

#[derive(Debug)]
pub struct WifiCredentials {
    pub ssid: String<32>,
    pub password: String<64>,
    pub hostname: String<32>,
}

#[derive(Debug)]
pub enum WifiCredTooLongError {
    Ssid,
    Password,
    Hostname,
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

pub fn get_default_wifi_credentials() -> Result<WifiCredentials, WifiCredTooLongError> {
    Ok(WifiCredentials {
        ssid: String::try_from(SSID).map_err(|_| WifiCredTooLongError::Ssid)?,
        password: String::try_from(PASSWORD).map_err(|_| WifiCredTooLongError::Password)?,
        hostname: String::try_from("esp-device").map_err(|_| WifiCredTooLongError::Hostname)?,
    })
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
    info!("Received new Wi-Fi settings:");
    info!("  • Hostname: {}", settings.hostname);
    info!("  • SSID:     {}", settings.ssid);
    info!("  • Password: {}", settings.psw);

    {
        let mut db = db_mutex.lock().await;
        write_db(&mut db, WIFI_HOSTNAME_KEY, settings.hostname.as_bytes()).await?;
        write_db(&mut db, WIFI_SSID_KEY, settings.ssid.as_bytes()).await?;
        write_db(&mut db, WIFI_PASSWORD_KEY, settings.psw.as_bytes()).await?;
    }

    let (len, ssid) = read_wifi_ssid(db_mutex).await?;
    if len == 0 {
        return Err(WifiSettingsError::InvalidData);
    }

    let verified = len == settings.ssid.len() && ssid.as_bytes() == settings.ssid.as_bytes();

    if verified {
        info!("✅  Wi-Fi settings saved and SSID verified.");
    } else {
        error!("⚠️  SSID verification FAILED!");
    }

    Ok(verified)
}

pub async fn read_wifi_ssid(db_mutex: &'static DbMutex) -> Result<(usize, String<32>), DbError> {
    read_setting(db_mutex, WIFI_SSID_KEY).await
}

pub async fn read_wifi_password(
    db_mutex: &'static DbMutex,
) -> Result<(usize, String<64>), DbError> {
    read_setting(db_mutex, WIFI_PASSWORD_KEY).await
}

pub async fn read_hostname(db_mutex: &'static DbMutex) -> Result<(usize, String<32>), DbError> {
    read_setting(db_mutex, WIFI_HOSTNAME_KEY).await
}
