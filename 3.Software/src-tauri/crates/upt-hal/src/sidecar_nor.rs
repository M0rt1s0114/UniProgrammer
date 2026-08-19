//! Minimal SPI NOR client over the `upt.hal` sidecar path.
//!
//! This module is intentionally Tauri-free. It wraps a [`HalRouter`] session
//! and translates the small set of SPI NOR operations used by the mock
//! sidecar adapter into plain `spi_transact` frames.

use crate::hal_router::HalRouter;
use crate::nor_ops;
use crate::spi_bus::SidecarSpiBus;

/// An open SPI NOR session routed through a sidecar adapter.
///
/// The session is opened by [`SidecarNor::open`]. In normal builds every
/// operation routes through [`SidecarSpiBus`], which opens and closes the HAL
/// session around each transaction, so [`SidecarNor::close`] is idempotent.
/// If the value is dropped without calling `close`, the underlying HAL session
/// (if one is open) stays open until [`HalRouter::shutdown`] closes every
/// remaining session.
pub struct SidecarNor<'a> {
    router: &'a mut HalRouter,
    adapter: String,
    device_id: String,
}

impl<'a> SidecarNor<'a> {
    /// Open a sidecar adapter device session and wrap it as a SPI NOR client.
    pub fn open(router: &'a mut HalRouter, adapter: &str, device_id: &str) -> Result<Self, String> {
        router.open(adapter, device_id)?;
        Ok(Self {
            router,
            adapter: adapter.to_string(),
            device_id: device_id.to_string(),
        })
    }

    fn bus(&mut self) -> SidecarSpiBus<'_> {
        SidecarSpiBus {
            router: self.router,
            adapter: self.adapter.clone(),
            device_id: self.device_id.clone(),
        }
    }

    /// Read the JEDEC ID via `[0x9F]`.
    pub fn read_id(&mut self) -> Result<[u8; 3], String> {
        let mut bus = self.bus();
        nor_ops::read_id(&mut bus)
    }

    /// Read `len` bytes from `addr` via `[0x03, a2, a1, a0]`.
    pub fn read(&mut self, addr: usize, len: usize) -> Result<Vec<u8>, String> {
        let mut bus = self.bus();
        nor_ops::read(&mut bus, addr, len)
    }

    /// Write-enable (`[0x06]`) and page-program (`[0x02]`) `data` at `addr`.
    ///
    /// The data must be non-empty and at most 256 bytes.
    pub fn program_page(&mut self, addr: usize, data: &[u8]) -> Result<(), String> {
        let mut bus = self.bus();
        nor_ops::page_program(&mut bus, addr, data)
    }

    /// Write-enable (`[0x06]`) and erase the entire chip (`[0xC7]`).
    pub fn erase_chip(&mut self) -> Result<(), String> {
        let mut bus = self.bus();
        nor_ops::erase_chip(&mut bus)
    }

    /// Write-enable (`[0x06]`) and erase the 4 KiB sector containing `addr`
    /// (`[0x20, a2, a1, a0]`).
    pub fn erase_sector(&mut self, addr: usize) -> Result<(), String> {
        let mut bus = self.bus();
        nor_ops::erase_sector(&mut bus, addr)
    }

    /// Read back `data` from `addr` and return a readable first-mismatch error.
    pub fn verify(&mut self, addr: usize, data: &[u8]) -> Result<(), String> {
        let mut bus = self.bus();
        nor_ops::verify(&mut bus, addr, data)
    }

    /// Close the underlying HAL session and release the router borrow.
    ///
    /// `SidecarSpiBus` transactions close the session after each operation,
    /// so `close` is intentionally idempotent: a missing session is treated
    /// as success.
    pub fn close(self) -> Result<(), String> {
        let has_session = self
            .router
            .sessions
            .iter()
            .any(|s| s.adapter == self.adapter && s.device_id == self.device_id);
        if has_session {
            self.router.close(&self.adapter, &self.device_id)?;
        }
        Ok(())
    }
}
