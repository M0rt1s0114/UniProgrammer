// SFDP (Serial Flash Discoverable Parameters) parsing, JEDEC JESD216.
//
// Adapted from ratchet (MIT), https://github.com/jackulau/ratchet
// (`rust/ratchet-core/src/sfdp.rs`). The parser is intentionally transport
// agnostic: callers provide a closure that performs an SFDP read
// (opcode 0x5A + 3-byte address + one dummy byte) at an SFDP-space offset.
//
// We only parse the mandatory header and Basic Flash Parameter Table; that is
// enough to size and program an otherwise unknown SPI NOR chip.

const SFDP_SIGNATURE: [u8; 4] = [0x53, 0x46, 0x44, 0x50]; // "SFDP"

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfdpParameterHeader {
    pub id_lsb: u8,
    pub minor_rev: u8,
    pub major_rev: u8,
    pub length: u8,
    pub table_pointer: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfdpHeaderInfo {
    pub valid: bool,
    pub minor_rev: u8,
    pub major_rev: u8,
    pub num_parameter_headers: u8,
    pub parameter_headers: Vec<SfdpParameterHeader>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraseType {
    pub size_bytes: u32,
    pub opcode: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfdpBasicFlashParams {
    pub erase_size_4kb: bool,
    pub fast_read_supported: bool,
    pub address_byte_count: AddressByteCount,
    pub fast_read_opcode: u8,
    pub density_bytes: u64,
    pub erase_types: Vec<EraseType>,
    pub page_size: u32,
    pub sector_size: u32,
    pub block_size: u32,
    pub needs_4byte_addr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressByteCount {
    Three,
    ThreeOrFour,
    Four,
}

fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

pub fn parse_sfdp_header(buf: &[u8]) -> SfdpHeaderInfo {
    let empty = SfdpHeaderInfo {
        valid: false,
        minor_rev: 0,
        major_rev: 0,
        num_parameter_headers: 0,
        parameter_headers: Vec::new(),
    };
    if buf.len() < 8 || buf[..4] != SFDP_SIGNATURE {
        return empty;
    }

    let minor_rev = buf[4];
    let major_rev = buf[5];
    let num_parameter_headers = buf[6].wrapping_add(1);
    let mut parameter_headers = Vec::new();
    for i in 0..num_parameter_headers as usize {
        let offset = 8 + i * 8;
        if buf.len() < offset + 8 {
            break;
        }
        parameter_headers.push(SfdpParameterHeader {
            id_lsb: buf[offset],
            minor_rev: buf[offset + 1],
            major_rev: buf[offset + 2],
            length: buf[offset + 3],
            table_pointer: (buf[offset + 4] as u32)
                | ((buf[offset + 5] as u32) << 8)
                | ((buf[offset + 6] as u32) << 16),
        });
    }

    SfdpHeaderInfo {
        valid: true,
        minor_rev,
        major_rev,
        num_parameter_headers,
        parameter_headers,
    }
}

pub fn parse_basic_flash_params(buf: &[u8]) -> Option<SfdpBasicFlashParams> {
    if buf.len() < 16 {
        return None;
    }

    let dw1 = read_u32_le(buf, 0);
    let dw2 = read_u32_le(buf, 4);

    let erase_size_4kb = (dw1 & 0x3) == 0x1;
    let address_byte_count = match (dw1 >> 17) & 0x3 {
        0 => AddressByteCount::Three,
        1 => AddressByteCount::ThreeOrFour,
        _ => AddressByteCount::Four,
    };
    let fast_read_supported = ((dw1 >> 16) & 0x1) != 0;
    let fast_read_opcode = ((dw1 >> 8) & 0xff) as u8;

    let density_bits: u64 = if dw2 & 0x8000_0000 != 0 {
        let n = dw2 & 0x7fff_ffff;
        if n >= 32 {
            return None;
        }
        1u64 << n
    } else {
        (dw2 as u64) + 1
    };
    if density_bits == 0 {
        return None;
    }
    let density_bytes = density_bits / 8;

    let mut erase_types = Vec::new();
    if buf.len() >= 36 {
        for i in 0..4 {
            let size_exp = buf[28 + i * 2];
            let opcode = buf[28 + i * 2 + 1];
            if size_exp > 0 && size_exp < 32 {
                erase_types.push(EraseType {
                    size_bytes: 1u32 << size_exp,
                    opcode,
                });
            }
        }
    }

    let mut page_size: u32 = 256;
    if buf.len() >= 44 {
        let dw11 = read_u32_le(buf, 40);
        let page_size_bits = (dw11 >> 4) & 0x0f;
        if page_size_bits > 0 && page_size_bits < 24 {
            page_size = 1u32 << page_size_bits;
        }
    }

    let sector_size = erase_types
        .iter()
        .map(|e| e.size_bytes)
        .min()
        .unwrap_or(if erase_size_4kb { 4096 } else { 65536 });
    let block_size = erase_types
        .iter()
        .map(|e| e.size_bytes)
        .max()
        .unwrap_or(65536);
    let needs_4byte_addr = density_bytes > 16 * 1024 * 1024;

    Some(SfdpBasicFlashParams {
        erase_size_4kb,
        fast_read_supported,
        address_byte_count,
        fast_read_opcode,
        density_bytes,
        erase_types,
        page_size,
        sector_size,
        block_size,
        needs_4byte_addr,
    })
}

/// Full JESD216 discovery over any transport. `read_at` performs an SFDP read
/// (0x5A + 3-byte address + dummy) at the given SFDP-space address. Returns
/// `Ok(None)` when the chip exposes no valid SFDP table.
pub fn discover_sfdp(
    mut read_at: impl FnMut(u32, usize) -> Result<Vec<u8>, String>,
) -> Result<Option<SfdpBasicFlashParams>, String> {
    let hdr = read_at(0, 16)?;
    let info = parse_sfdp_header(&hdr);
    if !info.valid {
        return Ok(None);
    }

    let Some(ph) = info.parameter_headers.iter().find(|h| h.id_lsb == 0x00) else {
        return Ok(None);
    };
    let table_len = (ph.length as usize * 4).max(16);
    let table = read_at(ph.table_pointer, table_len)?;
    Ok(parse_basic_flash_params(&table))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_table(density_mb: u32, page_shift: u8) -> Vec<u8> {
        let mut table = vec![0u8; 44];
        // DW1: fast read bit16 + opcode 0x0B, 3-byte addressing.
        let dw1 = (1u32 << 16) | (0x0B << 8) | 0x01; // 4KB erase supported
        table[0..4].copy_from_slice(&dw1.to_le_bytes());
        // DW2: density in bits, encoded as 2^n when bit31 is set.
        let bits = (density_mb as u64) * 8 * 1024 * 1024;
        let n = bits.trailing_zeros();
        let dw2 = 0x8000_0000u32 | n;
        table[4..8].copy_from_slice(&dw2.to_le_bytes());
        // Erase type 1: 4KB @ 0x20; erase type 2: 64KB @ 0xD8.
        table[28] = 12;
        table[29] = 0x20;
        table[30] = 16;
        table[31] = 0xD8;
        // DW11 page size at offset 40: 1 << page_shift in bits 4..8.
        let dw11 = (page_shift as u32) << 4;
        table[40..44].copy_from_slice(&dw11.to_le_bytes());
        table
    }

    fn synthetic_header(table_ptr: u32, table_len_words: u8) -> Vec<u8> {
        let mut buf = vec![0u8; 16];
        buf[..4].copy_from_slice(&SFDP_SIGNATURE);
        buf[4] = 0; // minor
        buf[5] = 1; // major
        buf[6] = 0; // nph = 1 header
        buf[7] = 0xFF; // unknown access protocol, not parsed
        buf[8] = 0x00; // basic flash parameter table id
        buf[11] = table_len_words;
        buf[12..15].copy_from_slice(&table_ptr.to_le_bytes()[..3]);
        buf
    }

    #[test]
    fn header_round_trip() {
        let buf = synthetic_header(0x30, 11);
        let info = parse_sfdp_header(&buf);
        assert!(info.valid);
        assert_eq!(info.num_parameter_headers, 1);
        assert_eq!(info.parameter_headers[0].table_pointer, 0x30);
    }

    #[test]
    fn rejects_bad_signature() {
        let mut buf = synthetic_header(0, 11);
        buf[0] = 0;
        assert!(!parse_sfdp_header(&buf).valid);
    }

    #[test]
    fn parses_basic_params_geometry() {
        let params = parse_basic_flash_params(&synthetic_table(8, 8)).expect("basic params");
        assert_eq!(params.density_bytes, 8 * 1024 * 1024);
        assert_eq!(params.page_size, 256);
        assert_eq!(params.sector_size, 4096);
        assert_eq!(params.block_size, 65536);
        assert!(params.fast_read_supported);
        assert_eq!(params.fast_read_opcode, 0x0B);
        assert!(!params.needs_4byte_addr);
    }

    #[test]
    fn density_exponent_drives_4byte_mode() {
        let params = parse_basic_flash_params(&synthetic_table(32, 8)).expect("basic params");
        assert_eq!(params.density_bytes, 32 * 1024 * 1024);
        assert!(params.needs_4byte_addr);
    }

    #[test]
    fn discover_reads_header_and_table() {
        let table = synthetic_table(16, 8);
        let mut header = synthetic_header(0x30, 11);
        header.extend_from_slice(&table);
        let found = discover_sfdp(|addr, len| match addr {
            0 => {
                assert_eq!(len, 16);
                Ok(header[..len].to_vec())
            }
            0x30 => {
                assert!(len >= table.len());
                Ok(table[..len].to_vec())
            }
            other => panic!("unexpected SFDP address 0x{other:X}"),
        })
        .expect("discover")
        .expect("valid sfdp");
        assert_eq!(found.density_bytes, 16 * 1024 * 1024);
    }
}
