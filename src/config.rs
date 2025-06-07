use crate::{DbMutex, KvDatabase};
use core::fmt;
use ekv::{CommitError, ReadError, WriteError};
use esp_storage::FlashStorageError;
use heapless::String;
use log::{error};

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
            error!("Truncation occurred for key {:?}", key);
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

pub async fn write_db(db: &mut KvDatabase, key: &[u8], value: &[u8]) -> DbResult<()> {
    let mut tx = db.write_transaction().await;
    tx.write(key, value).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn read_db(db: &mut KvDatabase, key: &[u8], buf: &mut [u8]) -> Result<usize, DbError> {
    let rtx = db.read_transaction().await;
    Ok(rtx.read(key, buf).await?)
}
