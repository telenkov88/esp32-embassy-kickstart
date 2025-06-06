use crate::config::{DbError, read_setting, write_db};
use crate::shared::or_str;
use crate::{DbMutex, KvDatabase};
use core::fmt;
use ekv::{CommitError, ReadError, WriteError};
use esp_storage::FlashStorageError;
use heapless::String;
use log::{error, info};
use serde::{Deserialize, Serialize};

const MQTT_BROKER_KEY: &[u8] = b"mqtt.broker";
const MQTT_CLIENT_ID_KEY: &[u8] = b"mqtt.client_id";
const MQTT_USERNAME_KEY: &[u8] = b"mqtt.username";
const MQTT_PASSWORD_KEY: &[u8] = b"mqtt.password";

pub const MQTT_BROKER: &str = or_str(option_env!("MQTT_BROKER"), "tcp://localhost:1883");
pub const MQTT_CLIENT_ID: &str = or_str(option_env!("MQTT_CLIENT_ID"), "esp32-client");
pub const MQTT_USERNAME: &str = or_str(option_env!("MQTT_USERNAME"), "");
pub const MQTT_PASSWORD: &str = or_str(option_env!("MQTT_PASSWORD"), "");

#[derive(Debug)]
pub struct MqttCredentials {
    pub broker_uri: String<256>,
    pub client_id: String<64>,
    pub username: String<64>,
    pub password: String<128>,
}

#[derive(Debug)]
pub enum MqttCredTooLongError {
    BrokerUri,
    ClientId,
    Username,
    Password,
}

#[derive(Debug)]
pub enum MqttSettingsError {
    Storage(DbError),
    InvalidData,
}

pub fn get_default_mqtt_credentials() -> Result<MqttCredentials, MqttCredTooLongError> {
    Ok(MqttCredentials {
        broker_uri: String::try_from(MQTT_BROKER).map_err(|_| MqttCredTooLongError::BrokerUri)?,
        client_id: String::try_from(MQTT_CLIENT_ID).map_err(|_| MqttCredTooLongError::ClientId)?,
        username: String::try_from(MQTT_USERNAME).map_err(|_| MqttCredTooLongError::Username)?,
        password: String::try_from(MQTT_PASSWORD).map_err(|_| MqttCredTooLongError::Password)?,
    })
}

pub async fn read_mqtt_broker(db_mutex: &'static DbMutex) -> Result<(usize, String<256>), DbError> {
    read_setting(db_mutex, MQTT_BROKER_KEY).await
}

pub async fn read_mqtt_client_id(
    db_mutex: &'static DbMutex,
) -> Result<(usize, String<64>), DbError> {
    read_setting(db_mutex, MQTT_CLIENT_ID_KEY).await
}

pub async fn read_mqtt_username(
    db_mutex: &'static DbMutex,
) -> Result<(usize, String<64>), DbError> {
    read_setting(db_mutex, MQTT_USERNAME_KEY).await
}

pub async fn read_mqtt_password(
    db_mutex: &'static DbMutex,
) -> Result<(usize, String<128>), DbError> {
    read_setting(db_mutex, MQTT_PASSWORD_KEY).await
}

pub async fn get_mqtt_credentials(
    db_mutex: &'static DbMutex,
) -> Result<MqttCredentials, MqttSettingsError> {
    let (_, broker_uri) = read_mqtt_broker(db_mutex)
        .await
        .map_err(MqttSettingsError::Storage)?;
    let (_, client_id) = read_mqtt_client_id(db_mutex)
        .await
        .map_err(MqttSettingsError::Storage)?;
    let (_, username) = read_mqtt_username(db_mutex)
        .await
        .map_err(MqttSettingsError::Storage)?;
    let (_, password) = read_mqtt_password(db_mutex)
        .await
        .map_err(MqttSettingsError::Storage)?;

    if !broker_uri.is_empty() {
        Ok(MqttCredentials {
            broker_uri,
            client_id,
            username,
            password,
        })
    } else {
        Err(MqttSettingsError::InvalidData)
    }
}

pub async fn update_mqtt_credentials(
    creds: &MqttCredentials,
    db_mutex: &'static DbMutex,
) -> Result<bool, MqttSettingsError> {
    info!("Updating MQTT credentials:");
    info!("  • Broker: {}", creds.broker_uri);
    info!("  • Client ID: {}", creds.client_id);
    info!("  • Username: {}", creds.username);

    {
        let mut db = db_mutex.lock().await;
        write_db(&mut db, MQTT_BROKER_KEY, creds.broker_uri.as_bytes())
            .await
            .map_err(MqttSettingsError::Storage)?;
        write_db(&mut db, MQTT_CLIENT_ID_KEY, creds.client_id.as_bytes())
            .await
            .map_err(MqttSettingsError::Storage)?;
        write_db(&mut db, MQTT_USERNAME_KEY, creds.username.as_bytes())
            .await
            .map_err(MqttSettingsError::Storage)?;
        write_db(&mut db, MQTT_PASSWORD_KEY, creds.password.as_bytes())
            .await
            .map_err(MqttSettingsError::Storage)?;
    }

    let (len, broker) = read_mqtt_broker(db_mutex)
        .await
        .map_err(MqttSettingsError::Storage)?;
    let verified =
        len == creds.broker_uri.len() && broker.as_bytes() == creds.broker_uri.as_bytes();

    if verified {
        info!("✅ MQTT credentials saved and verified");
    } else {
        error!("⚠️ MQTT broker verification FAILED!");
    }

    Ok(verified)
}
