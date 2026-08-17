//! Firmware file loading.
//!
//! The programmer accepts the image formats people actually use for SPI
//! flash work:
//!
//! - raw images (`bin`, `rom`, `img`, `fw`, `dump`, `eep`, `eeprom`,
//!   `nand`, `spi`, `flash`) loaded byte-for-byte;
//! - Intel HEX (`hex`, `ihex`, `mcs`);
//! - Motorola S-record (`srec`, `s19`, `s28`, `s37`, `mot`);
//! - Microsoft UF2 (`uf2`, recognised by magic regardless of extension).
//!
//! Files picked through the dialog's "All files" fallback are sniffed by
//! content (UF2 magic / Intel HEX start code / S-record start code) and
//! otherwise treated as raw images.

use std::path::Path;

const IHEX_EXTENSIONS: &[&str] = &["hex", "ihex", "mcs"];
const SREC_EXTENSIONS: &[&str] = &["srec", "s19", "s28", "s37", "mot"];

/// Hard cap for parsed formats (Intel HEX / S-record / UF2). Raw images are
/// loaded exactly like before; this only prevents a malformed record from
/// asking for an absurdly large in-memory image.
const MAX_PARSED_IMAGE: usize = 256 * 1024 * 1024;

const UF2_MAGIC_START0: u32 = 0x0A32_4655;
const UF2_MAGIC_START1: u32 = 0x9E5D_5157;
const UF2_MAGIC_END: u32 = 0x0AB1_6F30;
const UF2_BLOCK_SIZE: usize = 512;
const UF2_PAYLOAD_SIZE: usize = 476;

pub fn load_firmware_file(path: &str) -> Result<(Vec<u8>, &'static str), String> {
    let raw = std::fs::read(path).map_err(|e| format!("读取文件失败 {}: {}", path, e))?;
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    if IHEX_EXTENSIONS.contains(&ext.as_str()) {
        return parse_intel_hex(&raw).map(|bytes| (bytes, "Intel HEX"));
    }
    if SREC_EXTENSIONS.contains(&ext.as_str()) {
        return parse_srec(&raw).map(|bytes| (bytes, "Motorola S-record"));
    }
    if ext == "uf2" {
        return parse_uf2(&raw).map(|bytes| (bytes, "UF2"));
    }

    // Raw extension or "All files" fallback: sniff well-known containers so
    // a renamed file still decodes correctly.
    if looks_like_uf2(&raw) {
        return parse_uf2(&raw).map(|bytes| (bytes, "UF2"));
    }
    if looks_like_intel_hex(&raw) {
        return parse_intel_hex(&raw).map(|bytes| (bytes, "Intel HEX"));
    }
    if looks_like_srec(&raw) {
        // A raw image can theoretically start with `S` followed by a digit;
        // only treat it as S-record when the whole file parses.
        if let Ok(bytes) = parse_srec(&raw) {
            return Ok((bytes, "Motorola S-record"));
        }
    }
    Ok((raw, "原始镜像"))
}

fn trim_ascii(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|p| p + 1)
        .unwrap_or(start);
    &line[start..end]
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((hex_digit(hi)? << 4) | hex_digit(lo)?)
}

fn fill_to(image: &mut Vec<u8>, len: usize) -> Result<(), String> {
    if len > MAX_PARSED_IMAGE {
        return Err(format!(
            "固件镜像超过解析上限 {} MiB",
            MAX_PARSED_IMAGE / 1024 / 1024
        ));
    }
    if image.len() < len {
        image.resize(len, 0xFF);
    }
    Ok(())
}

// ───────────────────────────── Intel HEX ─────────────────────────────

fn looks_like_intel_hex(data: &[u8]) -> bool {
    trim_ascii(data).first() == Some(&b':')
}

fn parse_intel_hex(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut image: Vec<u8> = Vec::new();
    let mut linear_base: u64 = 0;
    let mut segment_base: u64 = 0;
    let mut saw_data = false;
    let mut saw_eof = false;

    for (line_no, raw_line) in data.split(|&b| b == b'\n').enumerate() {
        let line = trim_ascii(raw_line);
        if line.is_empty() {
            continue;
        }
        if saw_eof {
            break;
        }
        if line[0] != b':' {
            return Err(format!("Intel HEX 第 {} 行不是 ':' 记录", line_no + 1));
        }
        let body = &line[1..];
        if body.len() < 10 || !body.len().is_multiple_of(2) {
            return Err(format!("Intel HEX 第 {} 行记录长度无效", line_no + 1));
        }
        let count = hex_byte(body[0], body[1])
            .ok_or_else(|| format!("Intel HEX 第 {} 行字节计数无效", line_no + 1))?
            as usize;
        if body.len() != (count + 5) * 2 {
            return Err(format!(
                "Intel HEX 第 {} 行记录长度与字节计数不符",
                line_no + 1
            ));
        }

        let mut record = Vec::with_capacity(count + 5);
        for i in 0..count + 5 {
            record.push(
                hex_byte(body[i * 2], body[i * 2 + 1])
                    .ok_or_else(|| format!("Intel HEX 第 {} 行含非法十六进制字符", line_no + 1))?,
            );
        }
        if record.iter().map(|&b| b as u32).sum::<u32>() & 0xFF != 0 {
            return Err(format!("Intel HEX 第 {} 行校验和错误", line_no + 1));
        }

        let addr = ((record[1] as u64) << 8) | record[2] as u64;
        let rectype = record[3];
        let payload = &record[4..4 + count];
        match rectype {
            0x00 => {
                let start = (linear_base + segment_base + addr) as usize;
                fill_to(&mut image, start + count)?;
                image[start..start + count].copy_from_slice(payload);
                saw_data = true;
            }
            0x01 => saw_eof = true,
            0x02 => {
                if count != 2 {
                    return Err(format!(
                        "Intel HEX 第 {} 行扩展段地址记录长度应为 2",
                        line_no + 1
                    ));
                }
                segment_base = (((payload[0] as u16) << 8 | payload[1] as u16) as u64) << 4;
            }
            0x03 | 0x05 => {} // start address records: no image data
            0x04 => {
                if count != 2 {
                    return Err(format!(
                        "Intel HEX 第 {} 行扩展线性地址记录长度应为 2",
                        line_no + 1
                    ));
                }
                linear_base = (((payload[0] as u32) << 8 | payload[1] as u32) as u64) << 16;
            }
            other => {
                return Err(format!(
                    "Intel HEX 第 {} 行记录类型 0x{:02X} 不支持",
                    line_no + 1,
                    other
                ))
            }
        }
    }

    if !saw_data {
        return Err("Intel HEX 文件没有数据记录".into());
    }
    Ok(image)
}

// ────────────────────────── Motorola S-record ──────────────────────────

fn looks_like_srec(data: &[u8]) -> bool {
    let line = trim_ascii(data);
    matches!((line.first(), line.get(1)), (Some(b'S'), Some(b'0'..=b'9')))
}

fn parse_srec(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut image: Vec<u8> = Vec::new();
    let mut saw_data = false;

    for (line_no, raw_line) in data.split(|&b| b == b'\n').enumerate() {
        let line = trim_ascii(raw_line);
        if line.is_empty() {
            continue;
        }
        if line.first() != Some(&b'S') || !line.get(1).is_some_and(|b| b.is_ascii_digit()) {
            return Err(format!("S-record 第 {} 行不是有效记录", line_no + 1));
        }
        let rectype = line[1];
        let body = &line[2..];
        if body.len() < 2 {
            return Err(format!("S-record 第 {} 行记录过短", line_no + 1));
        }
        let count = hex_byte(body[0], body[1])
            .ok_or_else(|| format!("S-record 第 {} 行字节计数无效", line_no + 1))?
            as usize;
        if body.len() != 2 + count * 2 {
            return Err(format!(
                "S-record 第 {} 行记录长度与字节计数不符",
                line_no + 1
            ));
        }
        let mut record = Vec::with_capacity(count);
        for i in 0..count {
            record.push(
                hex_byte(body[2 + i * 2], body[3 + i * 2])
                    .ok_or_else(|| format!("S-record 第 {} 行含非法十六进制字符", line_no + 1))?,
            );
        }
        if (count as u32 + record.iter().map(|&b| b as u32).sum::<u32>()) & 0xFF != 0xFF {
            return Err(format!("S-record 第 {} 行校验和错误", line_no + 1));
        }

        match rectype {
            b'1' | b'2' | b'3' => {
                let addr_len = match rectype {
                    b'1' => 2,
                    b'2' => 3,
                    _ => 4,
                };
                if count < addr_len + 1 {
                    return Err(format!("S-record 第 {} 行地址长度不足", line_no + 1));
                }
                let mut addr: u64 = 0;
                for &b in &record[0..addr_len] {
                    addr = (addr << 8) | b as u64;
                }
                let payload = &record[addr_len..count - 1];
                let start = addr as usize;
                fill_to(&mut image, start + payload.len())?;
                image[start..start + payload.len()].copy_from_slice(payload);
                saw_data = true;
            }
            b'7' | b'8' | b'9' => break, // termination record
            b'0' | b'5' | b'6' => {}     // header / count records
            other => {
                return Err(format!(
                    "S-record 第 {} 行记录类型 S{} 不支持",
                    line_no + 1,
                    other as char
                ))
            }
        }
    }

    if !saw_data {
        return Err("S-record 文件没有数据记录".into());
    }
    Ok(image)
}

// ───────────────────────────── UF2 ─────────────────────────────

fn u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn looks_like_uf2(data: &[u8]) -> bool {
    data.len() >= 4 && u32_le(data, 0) == UF2_MAGIC_START0
}

fn parse_uf2(data: &[u8]) -> Result<Vec<u8>, String> {
    if !data.len().is_multiple_of(UF2_BLOCK_SIZE) {
        return Err(format!(
            "UF2 文件大小应为 {} 字节的整数倍（实际 {} 字节）",
            UF2_BLOCK_SIZE,
            data.len()
        ));
    }
    if data.is_empty() {
        return Err("UF2 文件为空".into());
    }

    struct Entry {
        block_no: u32,
        payload_start: usize,
        payload_len: usize,
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut declared_blocks: u32 = 0;
    for block_start in (0..data.len()).step_by(UF2_BLOCK_SIZE) {
        let block = &data[block_start..block_start + UF2_BLOCK_SIZE];
        if u32_le(block, 0) != UF2_MAGIC_START0
            || u32_le(block, 4) != UF2_MAGIC_START1
            || u32_le(block, UF2_BLOCK_SIZE - 4) != UF2_MAGIC_END
        {
            return Err(format!(
                "UF2 块 {} 魔数不匹配，不是有效的 UF2 文件",
                block_start / UF2_BLOCK_SIZE
            ));
        }
        let payload_len = u32_le(block, 16) as usize;
        if payload_len > UF2_PAYLOAD_SIZE {
            return Err(format!(
                "UF2 块 {} 载荷长度 {} 超过 {}",
                block_start / UF2_BLOCK_SIZE,
                payload_len,
                UF2_PAYLOAD_SIZE
            ));
        }
        let block_no = u32_le(block, 20);
        let num_blocks = u32_le(block, 24);
        if num_blocks != 0 {
            if declared_blocks != 0 && num_blocks != declared_blocks {
                return Err("UF2 各块声明的总块数不一致".into());
            }
            declared_blocks = num_blocks;
        }
        entries.push(Entry {
            block_no,
            payload_start: block_start + 32,
            payload_len,
        });
    }

    let max_block = entries.iter().map(|e| e.block_no).max().unwrap_or(0);
    let total_blocks = if declared_blocks > 0 {
        declared_blocks
    } else {
        max_block + 1
    } as usize;
    if total_blocks == 0 {
        return Err("UF2 文件没有可用的数据块".into());
    }
    let total_bytes = total_blocks.saturating_mul(256);
    if total_bytes > MAX_PARSED_IMAGE {
        return Err(format!(
            "UF2 展开后超过解析上限 {} MiB",
            MAX_PARSED_IMAGE / 1024 / 1024
        ));
    }

    let mut image = vec![0xFFu8; total_bytes];
    let mut seen = vec![false; total_blocks];
    for entry in &entries {
        let index = entry.block_no as usize;
        if index >= total_blocks {
            return Err(format!(
                "UF2 块号 {} 超出总块数 {}",
                entry.block_no, total_blocks
            ));
        }
        if seen[index] {
            return Err(format!("UF2 块号 {} 重复", entry.block_no));
        }
        seen[index] = true;
        let start = index * 256;
        image[start..start + entry.payload_len]
            .copy_from_slice(&data[entry.payload_start..entry.payload_start + entry.payload_len]);
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_record(data: &[u8], addr: u16, rectype: u8) -> String {
        let count = data.len() as u8;
        let mut bytes = vec![count, (addr >> 8) as u8, addr as u8, rectype];
        bytes.extend_from_slice(data);
        let sum = bytes.iter().map(|&b| b as u32).sum::<u32>();
        let checksum = ((!(sum & 0xFF) + 1) & 0xFF) as u8;
        bytes.push(checksum);
        let hex: String = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        format!(":{hex}\n")
    }

    #[test]
    fn intel_hex_basic_and_extended_linear() {
        let mut text = String::new();
        text.push_str(":020000040001F9\n"); // linear base 0x00010000
        text.push_str(&write_record(&[0x12, 0x34], 0x0100, 0x00));
        text.push_str(":00000001FF\n");
        let image = parse_intel_hex(text.as_bytes()).unwrap();
        assert_eq!(image.len(), 0x10102);
        assert_eq!(image[0], 0xFF);
        assert_eq!(image[0x10100], 0x12);
        assert_eq!(image[0x10101], 0x34);
    }

    #[test]
    fn intel_hex_rejects_bad_checksum() {
        let text = ":0400000012345678\n";
        assert!(parse_intel_hex(text.as_bytes()).is_err());
    }

    #[test]
    fn srec_s1_and_s3() {
        let text = "S10501001234B3\nS9030000FC\n";
        let image = parse_srec(text.as_bytes()).unwrap();
        assert_eq!(image.len(), 0x0102);
        assert_eq!(&image[0x0100..0x0102], &[0x12, 0x34]);
    }

    #[test]
    fn srec_rejects_bad_checksum() {
        assert!(parse_srec(b"S1070100123400\n").is_err());
    }

    #[test]
    fn uf2_round_trip() {
        let mut block = vec![0u8; UF2_BLOCK_SIZE];
        block[0..4].copy_from_slice(&UF2_MAGIC_START0.to_le_bytes());
        block[4..8].copy_from_slice(&UF2_MAGIC_START1.to_le_bytes());
        block[16..20].copy_from_slice(&8u32.to_le_bytes()); // payload size
        block[20..24].copy_from_slice(&0u32.to_le_bytes()); // block no
        block[24..28].copy_from_slice(&1u32.to_le_bytes()); // num blocks
        block[32..40].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x11, 0x22, 0x33]);
        block[UF2_BLOCK_SIZE - 4..].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());

        let image = parse_uf2(&block).unwrap();
        assert_eq!(image.len(), 256);
        assert_eq!(
            &image[0..8],
            &[0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x11, 0x22, 0x33]
        );
        assert_eq!(image[8], 0xFF);
        assert_eq!(image[255], 0xFF);
    }

    #[test]
    fn uf2_blocks_can_arrive_out_of_order() {
        let mut blocks = vec![0u8; UF2_BLOCK_SIZE * 2];
        for (n, start) in [(1usize, 0usize), (0usize, UF2_BLOCK_SIZE)] {
            let b = &mut blocks[start..start + UF2_BLOCK_SIZE];
            b[0..4].copy_from_slice(&UF2_MAGIC_START0.to_le_bytes());
            b[4..8].copy_from_slice(&UF2_MAGIC_START1.to_le_bytes());
            b[16..20].copy_from_slice(&256u32.to_le_bytes());
            b[20..24].copy_from_slice(&(n as u32).to_le_bytes());
            b[24..28].copy_from_slice(&2u32.to_le_bytes());
            b[32] = n as u8;
            b[UF2_BLOCK_SIZE - 4..].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());
        }
        let image = parse_uf2(&blocks).unwrap();
        assert_eq!(image[0], 0);
        assert_eq!(image[256], 1);
    }

    #[test]
    fn uf2_sniff_requires_start_magic() {
        let mut block = vec![0u8; UF2_BLOCK_SIZE];
        block[0..4].copy_from_slice(&UF2_MAGIC_START0.to_le_bytes());
        assert!(looks_like_uf2(&block));
        assert!(!looks_like_uf2(&[0x00, 0x01, 0x02, 0x03]));
        assert!(!looks_like_uf2(&[]));
    }

    fn valid_uf2_block(block_no: u32) -> Vec<u8> {
        let mut block = vec![0u8; UF2_BLOCK_SIZE];
        block[0..4].copy_from_slice(&UF2_MAGIC_START0.to_le_bytes());
        block[4..8].copy_from_slice(&UF2_MAGIC_START1.to_le_bytes());
        block[16..20].copy_from_slice(&4u32.to_le_bytes());
        block[20..24].copy_from_slice(&block_no.to_le_bytes());
        block[24..28].copy_from_slice(&1u32.to_le_bytes());
        block[32..36].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        block[UF2_BLOCK_SIZE - 4..].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());
        block
    }

    #[test]
    fn load_dispatch_uses_extension_and_magic_sniffing() {
        let dir =
            std::env::temp_dir().join(format!("uniprogrammer-fw-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let raw_path = dir.join("image.BIN");
        std::fs::write(&raw_path, [1u8, 2, 3, 4]).unwrap();
        let (bytes, format) = load_firmware_file(raw_path.to_str().unwrap()).unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
        assert_eq!(format, "原始镜像");

        let hex_path = dir.join("fw.HEX");
        std::fs::write(&hex_path, ":01000000AA55\n").unwrap();
        let (bytes, format) = load_firmware_file(hex_path.to_str().unwrap()).unwrap();
        assert_eq!(bytes, vec![0xAA]);
        assert_eq!(format, "Intel HEX");

        let srec_path = dir.join("fw.txt");
        std::fs::write(&srec_path, "S10501001234B3\nS9030000FC\n").unwrap();
        let (bytes, format) = load_firmware_file(srec_path.to_str().unwrap()).unwrap();
        assert_eq!(bytes.len(), 0x102);
        assert_eq!(format, "Motorola S-record");

        let uf2_path = dir.join("fw.dat");
        std::fs::write(&uf2_path, valid_uf2_block(0)).unwrap();
        let (bytes, format) = load_firmware_file(uf2_path.to_str().unwrap()).unwrap();
        assert_eq!(bytes.len(), 256);
        assert_eq!(format, "UF2");

        std::fs::remove_dir_all(&dir).ok();
    }
}
