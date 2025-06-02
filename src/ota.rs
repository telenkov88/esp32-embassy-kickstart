pub(crate) use esp_bootloader_esp_idf::ota::{Ota, OtaImageState, Slot};
use esp_bootloader_esp_idf::partitions::{
    self, AppPartitionSubType, DataPartitionSubType, PartitionTable, PartitionType,
};
use esp_println::println;
use esp_storage::FlashStorage;

pub fn run_with_ota<F, R>(
    flash_storage: &mut FlashStorage,
    partition_buf: &mut [u8; partitions::PARTITION_TABLE_MAX_LEN],
    operation: F,
) -> Result<R, partitions::Error>
where
    F: FnOnce(&mut Ota<FlashStorage>) -> R,
{
    let pt: PartitionTable = partitions::read_partition_table(flash_storage, partition_buf)?;
    println!("Partition table len: {:?}", pt.len());

    let ota_entry = pt
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))?
        .ok_or(partitions::Error::Invalid)?;

    let mut ota_storage = ota_entry.as_embedded_storage(flash_storage);
    let mut ota_handle = Ota::new(&mut ota_storage).map_err(|e| {
        println!("OTA init failed: {:?}", e);
        partitions::Error::Invalid
    })?;

    let ota0_part = pt.find_partition(PartitionType::App(AppPartitionSubType::Ota0))?;
    let ota0_offset = ota0_part.unwrap().offset();
    let ota1_part = pt.find_partition(PartitionType::App(AppPartitionSubType::Ota1))?;
    let ota1_offset = ota1_part.unwrap().offset();
    println!("Ota0 offset {}, Ota1 offset {}", ota0_offset, ota1_offset);
    println!("OTA initialized successfully");

    Ok(operation(&mut ota_handle))
}

#[allow(dead_code)]
pub fn set_next_ota_slot(next_slot: Slot, ota_handle: &mut Ota<FlashStorage>) {
    println!("Setting OTA slot to {:?}", next_slot);
    ota_handle.set_current_slot(next_slot).unwrap();
    ota_handle
        .set_current_ota_state(OtaImageState::New)
        .unwrap();
}

pub fn validate_current_ota_slot(ota_handle: &mut Ota<FlashStorage>) {
    let state = ota_handle.current_ota_state().unwrap();
    let slot = ota_handle.current_slot().unwrap();

    if slot != Slot::None && (state == OtaImageState::New || state == OtaImageState::PendingVerify)
    {
        println!("Marking current OTA slot as VALID");
        ota_handle
            .set_current_ota_state(OtaImageState::Valid)
            .unwrap();
    }
}
