//! Shared SPI bus abstraction for built-in and sidecar-backed SPI adapters.
//!
//! The [`SpiBus`] trait turns the three hardware paths used by this project
//! (CH34X programmers, serprog programmers, and sidecar adapter plugins) into
//! one full-duplex transaction primitive. Higher-level NOR logic can target
//! any of them without knowing which transport is underneath.

use crate::ch34x::Ch34xDevice;
use crate::hal_router::HalRouter;
use crate::serprog::Serprog;

/// A full-duplex SPI bus.
///
/// Implementations are expected to keep the semantics of the underlying
/// transport:
///
/// * `write` is the complete command + address + payload frame.
/// * `read_len` bytes are clocked back during (or immediately after) the
///   write, depending on the adapter.
/// * CS/start/stop handling is internal to the implementation, so callers do
///   not need to assert chip select manually.
pub trait SpiBus {
    /// Run one SPI transaction and return exactly `read_len` bytes.
    fn transact(&mut self, write: &[u8], read_len: usize) -> Result<Vec<u8>, String>;

    /// Maximum write-payload length (the full `write` frame) per transaction.
    fn max_write(&self) -> usize;

    /// Maximum read length per transaction.
    fn max_read(&self) -> usize;
}

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

/// [`SpiBus`] over a sidecar adapter plugin.
///
/// Each transaction opens a fresh HAL session, runs the SPI transaction, and
/// closes the session best-effort. If the SPI transaction succeeds, a close
/// error is ignored; if the transaction fails, close is still attempted and
/// the original transaction error is returned.
pub struct SidecarSpiBus<'a> {
    pub router: &'a mut HalRouter,
    pub adapter: String,
    pub device_id: String,
}

impl SpiBus for SidecarSpiBus<'_> {
    fn transact(&mut self, write: &[u8], read_len: usize) -> Result<Vec<u8>, String> {
        self.router.open(&self.adapter, &self.device_id)?;

        match self
            .router
            .spi_transact(&self.adapter, &self.device_id, write, read_len)
        {
            Ok(data) => {
                let _ = self.router.close(&self.adapter, &self.device_id);
                Ok(data)
            }
            Err(e) => {
                let _ = self.router.close(&self.adapter, &self.device_id);
                Err(e)
            }
        }
    }

    fn max_write(&self) -> usize {
        // v1 JSON/base64 transport practical write limit.
        4096
    }

    fn max_read(&self) -> usize {
        // v1 JSON/base64 transport practical read limit.
        65536
    }
}
