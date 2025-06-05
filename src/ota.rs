pub(crate) use esp_bootloader_esp_idf::ota::{Ota, OtaImageState, Slot};
use esp_bootloader_esp_idf::partitions::{
    self, AppPartitionSubType, DataPartitionSubType, PartitionTable, PartitionType,
};
use esp_storage::FlashStorage;
use log::{error, info};

pub type Error = partitions::Error;

pub fn run_with_ota<F, R>(
    flash: &mut FlashStorage,
    partition_buf: &mut [u8; partitions::PARTITION_TABLE_MAX_LEN],
    operation: F,
) -> Result<R, Error>
where
    F: FnOnce(&mut Ota<FlashStorage>) -> R,
{
    let pt: PartitionTable = partitions::read_partition_table(flash, partition_buf)?;
    info!("Partition table len: {}", pt.len());

    let ota_data = pt
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))?
        .ok_or(Error::Invalid)?;

    let mut ota_storage = ota_data.as_embedded_storage(flash);
    let mut ota = Ota::new(&mut ota_storage).map_err(|e| {
        error!("OTA init failed: {e:?}");
        Error::Invalid
    })?;

    let ota0_offset = pt
        .find_partition(PartitionType::App(AppPartitionSubType::Ota0))?
        .ok_or(Error::Invalid)?
        .offset();
    let ota1_offset = pt
        .find_partition(PartitionType::App(AppPartitionSubType::Ota1))?
        .ok_or(Error::Invalid)?
        .offset();
    info!("Ota0 offset {}, Ota1 offset {}", ota0_offset, ota1_offset);
    info!("OTA initialised successfully");

    Ok(operation(&mut ota))
}

#[allow(dead_code)]
pub fn set_next_ota_slot(next_slot: Slot, ota: &mut Ota<FlashStorage>) -> Result<(), Error> {
    info!("Setting OTA slot to {next_slot:?}");
    ota.set_current_slot(next_slot)?;
    ota.set_current_ota_state(OtaImageState::New)?;
    Ok(())
}

#[allow(dead_code)]
pub fn validate_current_ota_slot(ota: &mut Ota<FlashStorage>) -> Result<(), Error> {
    let state = ota.current_ota_state()?;
    let slot = ota.current_slot()?;

    if slot != Slot::None && matches!(state, OtaImageState::New | OtaImageState::PendingVerify) {
        info!("Marking current OTA slot as VALID");
        ota.set_current_ota_state(OtaImageState::Valid)?;
    }

    Ok(())
}
