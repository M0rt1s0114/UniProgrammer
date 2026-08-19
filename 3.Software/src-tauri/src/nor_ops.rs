//! Transport-agnostic SPI NOR command primitives.
//!
//! Every function in this module operates on a [`SpiBus`] implementation, so
//! the same NOR command logic works with built-in CH34X programmers, serprog
//! programmers, and sidecar adapter plugins.

use crate::spi_bus::SpiBus;

/// SPI NOR page-program payload limit.
pub const PAGE_SIZE: usize = 256;
/// Highest address representable by the 3-byte SPI NOR address field.
pub const ADDR_MASK_24: u32 = 0xFF_FFFF;

/// Convert a byte address into a 24-bit SPI NOR address field.
pub fn addr24(addr: usize) -> Result<u32, String> {
    let a =
        u32::try_from(addr).map_err(|_| format!("address {addr:#x} does not fit in 32 bits"))?;
    if a > ADDR_MASK_24 {
        return Err(format!(
            "address {addr:#x} exceeds the 24-bit SPI NOR address range"
        ));
    }
    Ok(a)
}

/// Read the JEDEC ID via `[0x9F]` and return its first three bytes.
pub fn read_id(bus: &mut dyn SpiBus) -> Result<[u8; 3], String> {
    let data = bus.transact(&[0x9F], 3)?;
    if data.len() < 3 {
        return Err(format!(
            "JEDEC ID read returned {} bytes, expected at least 3",
            data.len()
        ));
    }
    Ok([data[0], data[1], data[2]])
}

/// Read `len` bytes from `addr` via `[0x03, a2, a1, a0]`.
pub fn read(bus: &mut dyn SpiBus, addr: usize, len: usize) -> Result<Vec<u8>, String> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let a = addr24(addr)?;
    let write = [0x03, (a >> 16) as u8, (a >> 8) as u8, a as u8];
    let data = bus.transact(&write, len)?;
    if data.len() != len {
        return Err(format!(
            "SPI NOR READ returned {} bytes, expected {len}",
            data.len()
        ));
    }
    Ok(data)
}

/// Write-enable (`[0x06]`).
pub fn write_enable(bus: &mut dyn SpiBus) -> Result<(), String> {
    bus.transact(&[0x06], 0)?;
    Ok(())
}

/// Write-enable (`[0x06]`) and page-program (`[0x02]`) `data` at `addr`.
///
/// The data must be non-empty and at most [`PAGE_SIZE`] bytes.
pub fn page_program(bus: &mut dyn SpiBus, addr: usize, data: &[u8]) -> Result<(), String> {
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

    write_enable(bus)?;

    let mut write = Vec::with_capacity(4 + data.len());
    write.push(0x02);
    write.push((a >> 16) as u8);
    write.push((a >> 8) as u8);
    write.push(a as u8);
    write.extend_from_slice(data);
    bus.transact(&write, 0)?;
    Ok(())
}

/// Write-enable (`[0x06]`) and erase the entire chip (`[0xC7]`).
pub fn erase_chip(bus: &mut dyn SpiBus) -> Result<(), String> {
    write_enable(bus)?;
    bus.transact(&[0xC7], 0)?;
    Ok(())
}

/// Write-enable (`[0x06]`) and erase the 4 KiB sector containing `addr`
/// (`[0x20, a2, a1, a0]`).
pub fn erase_sector(bus: &mut dyn SpiBus, addr: usize) -> Result<(), String> {
    let a = addr24(addr)?;
    write_enable(bus)?;
    let write = [0x20, (a >> 16) as u8, (a >> 8) as u8, a as u8];
    bus.transact(&write, 0)?;
    Ok(())
}

/// Read back `data` from `addr` and return a readable first-mismatch error.
pub fn verify(bus: &mut dyn SpiBus, addr: usize, data: &[u8]) -> Result<(), String> {
    let actual = read(bus, addr, data.len())?;
    for (i, (&expected, &actual)) in data.iter().zip(actual.iter()).enumerate() {
        if expected != actual {
            return Err(format!(
                "校验失败 @ 0x{:08X}: 期望 0x{:02X}, 读到 0x{:02X}",
                addr + i,
                expected,
                actual
            ));
        }
    }
    Ok(())
}
