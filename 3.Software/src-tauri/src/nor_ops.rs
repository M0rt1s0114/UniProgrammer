//! Transport-agnostic SPI NOR command primitives.
//!
//! Every function in this module operates on a [`SpiBus`] implementation, so
//! the same NOR command logic works with built-in CH34X programmers, serprog
//! programmers, and sidecar adapter plugins.

use serde::Serialize;

use crate::spi_bus::SpiBus;

/// SPI NOR page-program payload limit.
pub const PAGE_SIZE: usize = 256;
/// Highest address representable by the 3-byte SPI NOR address field.
pub const ADDR_MASK_24: u32 = 0xFF_FFFF;
/// SR1 block-protect bits BP0..BP2.
///
/// A few parts keep BP4 in SR2; [`nor_bp_bits`] adds it when SR2 is valid.
pub const BP_MASK_SR1: u8 = 0x04 | 0x08 | 0x10;

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

/// Block-protect bits encoded as SR1 BP0..BP2 plus `0x20` for BP4 (SR2 bit 6)
/// when SR2 is valid (an all-`0xFF` SR2 read means the register does not
/// exist).
pub fn nor_bp_bits(sr1: u8, sr2: u8) -> u8 {
    (sr1 & BP_MASK_SR1)
        | if sr2 != 0xFF && (sr2 & 0x40) != 0 {
            0x20
        } else {
            0
        }
}

/// Raw status-register snapshot used by the generic write-protect commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NorWpStatus {
    pub sr1: u8,
    pub sr2: u8,
    pub sr3: u8,
    pub bp_bits: u8,
    pub write_protected: bool,
}

/// Read SR1/SR2/SR3 and report the current SPI NOR write-protection state.
///
/// SR1 is mandatory and its read error propagates. SR2 and SR3 are optional
/// on many parts, so a failed SR2/SR3 read is recorded as `0xFF`, matching
/// the built-in NOR path in `core`.
pub fn wp_status(bus: &mut dyn SpiBus) -> Result<NorWpStatus, String> {
    let sr1 = bus.transact(&[0x05], 1)?;
    let sr1 = *sr1
        .first()
        .ok_or_else(|| "SPI NOR SR1 read returned no data".to_string())?;
    let sr2 = bus
        .transact(&[0x35], 1)
        .ok()
        .and_then(|v| v.first().copied())
        .unwrap_or(0xFF);
    let sr3 = bus
        .transact(&[0x15], 1)
        .ok()
        .and_then(|v| v.first().copied())
        .unwrap_or(0xFF);
    let bp_bits = nor_bp_bits(sr1, sr2);
    Ok(NorWpStatus {
        sr1,
        sr2,
        sr3,
        bp_bits,
        write_protected: bp_bits != 0,
    })
}

/// Disable the SPI NOR block-protect bits and report the resulting status.
///
/// The sequence clears BP0..BP2 in SR1 via `WRSR` and, when SR2 is present
/// and BP4 (bit 6) is set, clears it via `WRSR2`. `EWSR` covers legacy parts
/// that predate WREN-gated `WRSR`; `WREN` covers modern parts, and both are
/// harmless on the other family.
///
/// Limitation: SRP (status-register-protect) and hardware WP# handling are
/// intentionally not implemented; on parts with those protections enabled the
/// register write may be ignored and the returned status will still show
/// `write_protected == true`.
pub fn wp_disable(bus: &mut dyn SpiBus) -> Result<NorWpStatus, String> {
    let before = wp_status(bus)?;
    if !before.write_protected {
        return Ok(before);
    }

    bus.transact(&[0x50], 0)?; // EWSR (legacy parts)
    bus.transact(&[0x06], 0)?; // WREN (modern parts)
    bus.transact(&[0x01, before.sr1 & !BP_MASK_SR1], 0)?; // WRSR

    if before.sr2 != 0xFF && (before.sr2 & 0x40) != 0 {
        bus.transact(&[0x06], 0)?; // WREN again
        bus.transact(&[0x31, before.sr2 & !0x40], 0)?; // WRSR2
    }

    wp_status(bus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nor_bp_bits_no_protection_returns_zero() {
        assert_eq!(nor_bp_bits(0x00, 0x00), 0);
        assert_eq!(nor_bp_bits(0xE3, 0x00), 0);
    }

    #[test]
    fn nor_bp_bits_masks_sr1_bp0_bp2() {
        assert_eq!(nor_bp_bits(0x04, 0x00), 0x04);
        assert_eq!(nor_bp_bits(0x08, 0x00), 0x08);
        assert_eq!(nor_bp_bits(0x10, 0x00), 0x10);
        assert_eq!(nor_bp_bits(0x1C, 0x00), BP_MASK_SR1);
        assert_eq!(nor_bp_bits(0xFF, 0x00), BP_MASK_SR1);
    }

    #[test]
    fn nor_bp_bits_adds_bp4_when_sr2_valid() {
        let sr1_bp = 0x04;
        assert_eq!(nor_bp_bits(sr1_bp, 0x40), 0x20 | sr1_bp);
        assert_eq!(nor_bp_bits(0x1C, 0x40), 0x20 | 0x1C);
        assert_eq!(nor_bp_bits(sr1_bp, 0x42), 0x20 | sr1_bp);
    }

    #[test]
    fn nor_bp_bits_ignores_sr2_ff() {
        assert_eq!(nor_bp_bits(0x1C, 0xFF), 0x1C);
        assert_eq!(nor_bp_bits(0x00, 0xFF), 0);
    }
}
