//! Minimal SPI NOR client over the `uni.hal` sidecar path.
//!
//! This module is intentionally Tauri-free. It wraps a [`HalRouter`] session
//! and translates the small set of SPI NOR operations used by the mock
//! sidecar adapter into plain `spi_transact` frames.

use crate::hal_router::HalRouter;

const PAGE_SIZE: usize = 256;
const ADDR_MASK_24: u32 = 0xFF_FFFF;

/// An open SPI NOR session routed through a sidecar adapter.
///
/// The session is opened by [`SidecarNor::open`] and is closed when
/// [`SidecarNor::close`] is called. If the value is dropped without calling
/// `close`, the underlying HAL session stays open until [`HalRouter::shutdown`]
/// closes every remaining session.
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

    /// Read the JEDEC ID via `[0x9F]`.
    pub fn read_id(&mut self) -> Result<[u8; 3], String> {
        let data = self
            .router
            .spi_transact(&self.adapter, &self.device_id, &[0x9F], 3)?;
        if data.len() < 3 {
            return Err(format!(
                "JEDEC ID read returned {} bytes, expected at least 3",
                data.len()
            ));
        }
        Ok([data[0], data[1], data[2]])
    }

    /// Read `len` bytes from `addr` via `[0x03, a2, a1, a0]`.
    pub fn read(&mut self, addr: usize, len: usize) -> Result<Vec<u8>, String> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let a = addr24(addr)?;
        let write = [0x03, (a >> 16) as u8, (a >> 8) as u8, a as u8];
        let data = self
            .router
            .spi_transact(&self.adapter, &self.device_id, &write, len)?;
        if data.len() != len {
            return Err(format!(
                "SPI NOR READ returned {} bytes, expected {len}",
                data.len()
            ));
        }
        Ok(data)
    }

    /// Write-enable (`[0x06]`) and page-program (`[0x02]`) `data` at `addr`.
    ///
    /// The data must be non-empty and at most 256 bytes.
    pub fn program_page(&mut self, addr: usize, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Err("program data must not be empty".to_string());
        }
        if data.len() > PAGE_SIZE {
            return Err(format!(
                "program data too long: {} bytes (max {PAGE_SIZE})",
                data.len()
            ));
        }
        let a = addr24(addr)?;

        self.router
            .spi_transact(&self.adapter, &self.device_id, &[0x06], 0)?;

        let mut write = Vec::with_capacity(4 + data.len());
        write.push(0x02);
        write.push((a >> 16) as u8);
        write.push((a >> 8) as u8);
        write.push(a as u8);
        write.extend_from_slice(data);
        self.router
            .spi_transact(&self.adapter, &self.device_id, &write, 0)?;
        Ok(())
    }

    /// Write-enable (`[0x06]`) and erase the entire chip (`[0xC7]`).
    pub fn erase_chip(&mut self) -> Result<(), String> {
        self.router
            .spi_transact(&self.adapter, &self.device_id, &[0x06], 0)?;
        self.router
            .spi_transact(&self.adapter, &self.device_id, &[0xC7], 0)?;
        Ok(())
    }

    /// Write-enable (`[0x06]`) and erase the 4 KiB sector containing `addr`
    /// (`[0x20, a2, a1, a0]`).
    pub fn erase_sector(&mut self, addr: usize) -> Result<(), String> {
        let a = addr24(addr)?;
        self.router
            .spi_transact(&self.adapter, &self.device_id, &[0x06], 0)?;
        let write = [0x20, (a >> 16) as u8, (a >> 8) as u8, a as u8];
        self.router
            .spi_transact(&self.adapter, &self.device_id, &write, 0)?;
        Ok(())
    }

    /// Read back `data` from `addr` and return a readable first-mismatch error.
    pub fn verify(&mut self, addr: usize, data: &[u8]) -> Result<(), String> {
        let actual = self.read(addr, data.len())?;
        for (i, (&expected, &actual)) in data.iter().zip(actual.iter()).enumerate() {
            if expected != actual {
                return Err(format!(
                    "verify mismatch at address {:#06x}: expected {:#04x}, read {:#04x}",
                    addr + i,
                    expected,
                    actual
                ));
            }
        }
        Ok(())
    }

    /// Close the underlying HAL session and release the router borrow.
    pub fn close(self) -> Result<(), String> {
        self.router.close(&self.adapter, &self.device_id)
    }
}

fn addr24(addr: usize) -> Result<u32, String> {
    let a =
        u32::try_from(addr).map_err(|_| format!("address {addr:#x} does not fit in 32 bits"))?;
    if a > ADDR_MASK_24 {
        return Err(format!(
            "address {addr:#x} exceeds the 24-bit SPI NOR address range"
        ));
    }
    Ok(a)
}
