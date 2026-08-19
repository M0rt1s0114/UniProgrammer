//! Shared SPI bus abstraction for sidecar-backed SPI adapters.
//!
//! The [`SpiBus`] trait turns the hardware paths used by the HAL crate
//! (sidecar adapter plugins, and any app-local implementations such as
//! CH34X or serprog programmers) into one full-duplex transaction primitive.
//! Higher-level NOR logic can target any of them without knowing which
//! transport is underneath.

use crate::hal_router::HalRouter;

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
