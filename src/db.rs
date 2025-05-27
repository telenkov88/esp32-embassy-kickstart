#[allow(unused_imports)]
use esp_backtrace as _;

use embedded_storage_async::nor_flash::{NorFlash as AsyncNorFlash, ReadNorFlash};
use ekv::flash::{self, PageID};
use ekv::{config};

#[repr(C, align(4))]
struct AlignedBuf<const N: usize>([u8; N]);

pub struct DbFlash<T: AsyncNorFlash + ReadNorFlash> {
    pub(crate) start: usize,
    pub(crate) flash: T,
}

impl<T: AsyncNorFlash + ReadNorFlash> flash::Flash for DbFlash<T> {
    type Error = T::Error;

    fn page_count(&self) -> usize {
        config::MAX_PAGE_COUNT
    }

    async fn erase(&mut self, page_id: PageID) -> Result<(), Self::Error> {
        embedded_storage_async::nor_flash::NorFlash::erase(&mut self.flash, (self.start + page_id.index() * config::PAGE_SIZE) as u32, (self.start + page_id.index() * config::PAGE_SIZE + config::PAGE_SIZE) as u32).await
    }

    async fn read(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &mut [u8],
    ) -> Result<(), Self::Error> {
        let address = self.start + page_id.index() * config::PAGE_SIZE + offset;
        let mut buf = AlignedBuf([0; config::PAGE_SIZE]);
        ReadNorFlash::read(&mut self.flash, address as u32, &mut buf.0[..data.len()]).await?;
        data.copy_from_slice(&buf.0[..data.len()]);
        Ok(())
    }

    async fn write(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        let address = self.start + page_id.index() * config::PAGE_SIZE + offset;
        let mut buf = AlignedBuf([0; config::PAGE_SIZE]);
        buf.0[..data.len()].copy_from_slice(data);
        embedded_storage_async::nor_flash::NorFlash::write(&mut self.flash, address as u32, &buf.0[..data.len()]).await
    }
}