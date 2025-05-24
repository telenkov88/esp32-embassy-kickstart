use embedded_storage::{ReadStorage, Storage};
use esp_bootloader_esp_idf::ota::Slot;
use esp_storage::FlashStorage;
use esp_bootloader_esp_idf::partitions;
use esp_bootloader_esp_idf::partitions::{DataPartitionSubType, PartitionEntry, PartitionTable};
use esp_hal::system::Stack;
use static_cell::StaticCell;

static mut PT_MEM: Stack<8192> = Stack::new();
static PT_STORAGE: StaticCell<[u8; partitions::PARTITION_TABLE_MAX_LEN]> = StaticCell::new();

pub struct OtaUpdate {
    /// Parsed view of the partition table.
    pt: PartitionTable<'static>,
}

impl OtaUpdate {
    /// Returns a raw pointer to the partition that the new app is/will be written to.
    /// 
    /// 
    pub fn new(flash: &mut FlashStorage) -> Result<Self, partitions::Error> {
        // Stable backing store living for the whole program
        let buf = PT_STORAGE.init([0u8; partitions::PARTITION_TABLE_MAX_LEN]);
        let pt = partitions::read_partition_table(flash, buf)?;
        Ok(Self { pt })
    }

    pub fn current_slot(
        &mut self,
        flash: &mut FlashStorage,
    ) -> Result<Slot, partitions::Error> {
        // ---- 1. find the *data/ota* partition entry ------------------------
        let ota_data_entry = self
            .pt
            .find_partition(partitions::PartitionType::Data(
                DataPartitionSubType::Ota,
            ))?
            .ok_or(partitions::Error::Invalid)?;

        // ---- 2. turn it into a FlashRegion that borrows `flash` ------------
        let mut region = ota_data_entry.as_embedded_storage(flash);

        // ---- 3. create a *temporary* Ota helper and query it ---------------
        let mut ota = esp_bootloader_esp_idf::ota::Ota::new(&mut region)?;
        ota.current_slot()
    }
}