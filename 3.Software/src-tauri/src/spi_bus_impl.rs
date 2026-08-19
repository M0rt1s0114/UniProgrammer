//! App-local [`SpiBus`] implementations for the built-in CH34X and serprog
//! programmers.
//!
//! The [`SpiBus`] trait itself lives in `uni-hal`; these implementations stay
//! in the app crate because they wrap app-local device types (`Ch34xDevice`
//! and `Serprog`) and orphan rules would prevent implementing the external
//! trait for those local types outside this crate.

use uni_devices::ch34x::Ch34xDevice;
use uni_devices::serprog::Serprog;
use uni_hal::spi_bus::SpiBus;

/// [`SpiBus`] over a built-in CH34X programmer.
///
/// CS is asserted around the whole transaction (`cs_low` -> `spi_tx` ->
/// `spi_rx` -> `cs_high`), matching the manual CS pattern used by the
/// existing CH34X callers. Errors propagate immediately; if `spi_tx` fails,
/// `cs_high` is *not* attempted.
pub struct Ch34xSpiBus<'a> {
    pub dev: &'a mut Ch34xDevice,
}

impl SpiBus for Ch34xSpiBus<'_> {
    fn transact(&mut self, write: &[u8], read_len: usize) -> Result<Vec<u8>, String> {
        self.dev.cs_low()?;
        self.dev.spi_tx(write)?;

        if read_len == 0 {
            self.dev.cs_high()?;
            return Ok(Vec::new());
        }

        let mut read = vec![0u8; read_len];
        self.dev.spi_rx(&mut read)?;
        self.dev.cs_high()?;
        Ok(read)
    }

    fn max_write(&self) -> usize {
        self.dev.spi_frame_limit()
    }

    fn max_read(&self) -> usize {
        self.dev.spi_frame_limit()
    }
}

/// [`SpiBus`] over a built-in serprog programmer.
pub struct SerprogSpiBus<'a> {
    pub dev: &'a mut Serprog,
}

impl SpiBus for SerprogSpiBus<'_> {
    fn transact(&mut self, write: &[u8], read_len: usize) -> Result<Vec<u8>, String> {
        self.dev.spi_command(write, read_len)
    }

    fn max_write(&self) -> usize {
        self.dev.max_write_len()
    }

    fn max_read(&self) -> usize {
        self.dev.max_read_len()
    }
}
