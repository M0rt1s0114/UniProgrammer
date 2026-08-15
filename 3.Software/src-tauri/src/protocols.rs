//! Chip-level command sequences ported from IMSProg.
//!
//! All functions assume an already-open `Ch34xDevice` in the correct bus mode
//! (see `bus_mode_for_protocol`). CS/START/STOP are driven manually exactly
//! like IMSProg. The device is opened and closed per operation by the caller.

use crate::ch34x::{Ch34xDevice, DeviceMode};
use crate::chiplib::ChipInfo;
use serde::Serialize;
use std::time::{Duration, Instant};

pub struct ChipParams {
    #[allow(dead_code)]
    pub protocol: String,
    pub page: usize,
    pub sector: usize,
    pub block: usize,
    pub algorithm: u8,
    #[allow(dead_code)]
    pub addr4bit: u8,
    #[allow(dead_code)]
    pub spare: usize,
}

impl ChipParams {
    pub fn from_info(info: &ChipInfo) -> Self {
        let addr4bit = info.attr_u32("addr4bit").unwrap_or(0) as u8;
        let mut block = info.attr_u64("block").unwrap_or(0) as usize;
        if block == 0 {
            block = match info.protocol.as_str() {
                "SPI_NOR" | "SPI_NAND" => 64 * 1024,
                _ => 0,
            };
        }
        ChipParams {
            protocol: info.protocol.clone(),
            page: info.page.max(1) as usize,
            sector: info.attr_u64("sector").unwrap_or(info.page as u64).max(1) as usize,
            block,
            algorithm: info.attr_u32("algorithm").unwrap_or(0) as u8,
            addr4bit,
            spare: info.attr_u64("spare").unwrap_or(0) as usize,
        }
    }

    #[allow(dead_code)]
    pub fn addr4b(&self) -> bool {
        (self.addr4bit & 0x0F) != 0
    }

    /// 4-byte address algorithm: 0 = B7/E9, 1 = Winbond, 2 = Spansion.
    #[allow(dead_code)]
    pub fn addr4b_alg(&self) -> u8 {
        (self.addr4bit >> 4) & 0x0F
    }
}

pub fn bus_mode_for_protocol(protocol: &str) -> DeviceMode {
    match protocol {
        "I2C" | "I2C_F-RAM" | "I2C_SPD" => DeviceMode::I2c,
        "Microwire" => DeviceMode::Microwire,
        _ => DeviceMode::Spi,
    }
}

// ═══════════════════════════ shared SPI helpers ═════════════════════════════

fn cs_cmd(dev: &Ch34xDevice, cmd: &[u8]) -> Result<(), String> {
    dev.cs_low()?;
    dev.spi_tx(cmd)?;
    dev.cs_high()
}

#[allow(dead_code)]
fn read_sr(dev: &Ch34xDevice) -> Result<u8, String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x05])?;
    let mut sr = [0xFFu8; 1];
    dev.spi_rx(&mut sr)?;
    dev.cs_high()?;
    Ok(sr[0])
}

#[allow(dead_code)]
fn wait_ready(dev: &Ch34xDevice, timeout_ms: u64) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let sr = read_sr(dev)?;
        if (sr & (0x01 | 0x02)) == 0 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("等待芯片就绪超时".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ═════════════════════════════ SPI EEPROM (s95) ═════════════════════════════

fn s95_read_sr(dev: &Ch34xDevice) -> Result<u8, String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x05])?;
    let mut sr = [0xFFu8; 1];
    dev.spi_rx(&mut sr)?;
    dev.cs_high()?;
    Ok(sr[0])
}

fn s95_wait_ready(dev: &Ch34xDevice, timeout_ms: u64) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let sr = s95_read_sr(dev)?;
        if (sr & (0x01 | 0x02)) == 0 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("等待 EEPROM 就绪超时".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn s95_write_enable(dev: &Ch34xDevice) -> Result<(), String> {
    cs_cmd(dev, &[0x06])
}

fn s95_write_disable(dev: &Ch34xDevice) -> Result<(), String> {
    cs_cmd(dev, &[0x04])
}

fn s95_unprotect(dev: &Ch34xDevice) -> Result<(), String> {
    let sr = s95_read_sr(dev)?;
    if (sr & (0x04 | 0x08)) != 0 {
        dev.cs_low()?;
        dev.spi_tx(&[0x01, 0x00])?;
        dev.cs_high()?;
        s95_wait_ready(dev, 1000)?;
    }
    Ok(())
}

pub fn s95_read(
    dev: &Ch34xDevice,
    params: &ChipParams,
    from: u64,
    len: usize,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let alg = params.algorithm & 0x0F;
    let a8 = params.algorithm & 0x10;
    let sector = params.sector.max(1);
    let max_chunk = dev.spi_frame_limit().saturating_sub(5).max(1);
    s95_wait_ready(dev, 1000)?;

    let total = len as u64;
    let mut out = Vec::with_capacity(len);
    let mut addr = from;
    let mut done = 0usize;

    while done < len {
        let chunk = (len - done).min(sector).min(max_chunk);
        dev.cs_low()?;
        if from > 255 && a8 > 0 {
            dev.spi_tx(&[0x0B])?;
        } else {
            dev.spi_tx(&[0x03])?;
        }
        let mut cmd = Vec::with_capacity(4);
        if alg == 2 {
            cmd.push(((addr >> 16) & 0xFF) as u8);
        }
        if alg > 0 {
            cmd.push(((addr >> 8) & 0xFF) as u8);
        }
        cmd.push((addr & 0xFF) as u8);
        dev.spi_tx(&cmd)?;
        let start = out.len();
        out.resize(start + chunk, 0xFF);
        dev.spi_rx(&mut out[start..start + chunk])?;
        dev.cs_high()?;

        addr += chunk as u64;
        done += chunk;
        progress(done as u64, total);
    }
    Ok(out)
}

pub fn s95_write(
    dev: &Ch34xDevice,
    params: &ChipParams,
    data: &[u8],
    start_addr: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let alg = params.algorithm & 0x0F;
    let a8 = params.algorithm & 0x10;
    let sector = params.sector.max(1);
    let max_chunk = dev.spi_frame_limit().saturating_sub(5).max(1);
    let total = data.len() as u64;
    let mut offset = 0usize;
    let mut addr = start_addr;

    s95_wait_ready(dev, 2000)?;
    while offset < data.len() {
        let chunk = (data.len() - offset).min(sector).min(max_chunk);
        s95_wait_ready(dev, 2000)?;
        s95_write_enable(dev)?;
        s95_unprotect(dev)?;

        dev.cs_low()?;
        if addr > 255 && a8 > 0 {
            dev.spi_tx(&[0x0A])?;
        } else {
            dev.spi_tx(&[0x02])?;
        }
        let mut cmd = Vec::with_capacity(4);
        if alg == 2 {
            cmd.push(((addr >> 16) & 0xFF) as u8);
        }
        if alg > 0 {
            cmd.push(((addr >> 8) & 0xFF) as u8);
        }
        cmd.push((addr & 0xFF) as u8);
        dev.spi_tx(&cmd)?;
        dev.spi_tx(&data[offset..offset + chunk])?;
        dev.cs_high()?;
        s95_wait_ready(dev, 100)?;

        addr += chunk as u64;
        offset += chunk;
        progress(offset as u64, total);
    }
    s95_write_disable(dev)
}

pub fn s95_erase(dev: &Ch34xDevice) -> Result<(), String> {
    s95_wait_ready(dev, 2000)?;
    s95_write_enable(dev)?;
    s95_unprotect(dev)?;
    cs_cmd(dev, &[0x62])?;
    s95_wait_ready(dev, 120_000)?;
    s95_write_disable(dev)
}

// ═════════════════════════════ DataFlash AT45 ═══════════════════════════════

fn at45_read_sr(dev: &Ch34xDevice) -> Result<u8, String> {
    dev.cs_low()?;
    dev.spi_tx(&[0xD7])?;
    let mut sr = [0u8; 1];
    dev.spi_rx(&mut sr)?;
    dev.cs_high()?;
    Ok(sr[0])
}

fn at45_wait_ready(dev: &Ch34xDevice, timeout_ms: u64) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let sr = at45_read_sr(dev)?;
        if (sr & 0x80) != 0 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("等待 DataFlash 就绪超时".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn at45_page_addr(params: &ChipParams, byte_addr: u64) -> Vec<u8> {
    let mut addr_len = 9u32;
    if params.sector > 511 {
        addr_len += 1;
    }
    let sector = params.sector.max(1) as u64;
    let page = byte_addr / sector;
    let off = byte_addr % sector;
    let physical = off + (page << addr_len);
    vec![
        ((physical >> 16) & 0xFF) as u8,
        ((physical >> 8) & 0xFF) as u8,
        (physical & 0xFF) as u8,
    ]
}

pub fn at45_read(
    dev: &Ch34xDevice,
    params: &ChipParams,
    from: u64,
    len: usize,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let sector = params.sector.max(1);
    let max_chunk = dev.spi_frame_limit().saturating_sub(8).max(1);
    at45_wait_ready(dev, 1000)?;
    let total = len as u64;
    let mut out = Vec::with_capacity(len);
    let mut addr = from;
    let mut done = 0usize;

    while done < len {
        let chunk = (len - done).min(sector).min(max_chunk);
        dev.cs_low()?;
        let mut cmd = vec![0xE8u8];
        cmd.extend(at45_page_addr(params, addr));
        cmd.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        dev.spi_tx(&cmd)?;
        let start = out.len();
        out.resize(start + chunk, 0xFF);
        dev.spi_rx(&mut out[start..start + chunk])?;
        dev.cs_high()?;

        addr += chunk as u64;
        done += chunk;
        progress(done as u64, total);
    }
    Ok(out)
}

pub fn at45_write(
    dev: &Ch34xDevice,
    params: &ChipParams,
    data: &[u8],
    start_addr: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let sector = params.sector.max(1);
    let max_chunk = dev.spi_frame_limit().saturating_sub(8).max(1);
    at45_wait_ready(dev, 1000)?;
    let total = data.len() as u64;
    let mut addr = start_addr;
    let mut offset = 0usize;

    while offset < data.len() {
        let chunk = (data.len() - offset).min(sector).min(max_chunk);
        at45_wait_ready(dev, 2000)?;
        dev.cs_low()?;
        let mut cmd = vec![0x82u8];
        cmd.extend(at45_page_addr(params, addr));
        dev.spi_tx(&cmd)?;
        dev.spi_tx(&data[offset..offset + chunk])?;
        dev.cs_high()?;
        at45_wait_ready(dev, 100)?;

        addr += chunk as u64;
        offset += chunk;
        progress(offset as u64, total);
    }
    Ok(())
}

/// Read the AT45 status register byte. Bit 0 reports the configured page
/// size (experimental interpretation: 1 = power-of-two binary page).
pub fn at45_read_page_mode(dev: &Ch34xDevice) -> Result<u8, String> {
    at45_read_sr(dev)
}

/// Configure the AT45 page size. `binary = true` issues 3Dh/2Ah/80h/A6h
/// (power-of-two page, e.g. 256/512 bytes); `binary = false` issues
/// 3Dh/2Ah/80h/A7h (standard DataFlash page, e.g. 264/528 bytes).
/// The setting is nonvolatile and requires hardware validation.
pub fn at45_set_page_mode(dev: &Ch34xDevice, binary: bool) -> Result<(), String> {
    at45_wait_ready(dev, 2000)?;
    let last = if binary { 0xA6 } else { 0xA7 };
    cs_cmd(dev, &[0x3D, 0x2A, 0x80, last])?;
    at45_wait_ready(dev, 5000)
}

pub fn at45_erase(dev: &Ch34xDevice) -> Result<(), String> {
    at45_wait_ready(dev, 2000)?;
    cs_cmd(dev, &[0xC7, 0x94, 0x80, 0x9A])?;
    at45_wait_ready(dev, 120_000)
}

// ═════════════════════════════ SPI NAND ═════════════════════════════════════

fn nand_read_main_sr(dev: &Ch34xDevice) -> Result<u8, String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x0F, 0xC0])?;
    let mut sr = [0u8; 1];
    dev.spi_rx(&mut sr)?;
    dev.cs_high()?;
    Ok(sr[0])
}

fn nand_wait_ready(dev: &Ch34xDevice, timeout_ms: u64) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let sr = nand_read_main_sr(dev)?;
        if (sr & 0x01) == 0 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("等待 NAND 就绪超时".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn nand_write_enable(dev: &Ch34xDevice) -> Result<(), String> {
    cs_cmd(dev, &[0x06])
}

fn nand_page_read(dev: &Ch34xDevice, page_size: usize, page_no: u32) -> Result<Vec<u8>, String> {
    nand_wait_ready(dev, 200)?;
    cs_cmd(
        dev,
        &[
            0x13,
            ((page_no >> 16) & 0xFF) as u8,
            ((page_no >> 8) & 0xFF) as u8,
            (page_no & 0xFF) as u8,
        ],
    )?;
    nand_wait_ready(dev, 200)?;

    let mut buf = vec![0xFFu8; page_size];
    if 4 + page_size <= dev.spi_frame_limit() {
        dev.cs_low()?;
        dev.spi_tx(&[0x03, 0x00, 0x00, 0x00])?;
        dev.spi_rx(&mut buf)?;
        dev.cs_high()?;
        return Ok(buf);
    }

    // DLL 后端单帧较小时，分多次 0x03 + 列地址 读取缓冲区（读操作可安全分段）
    let chunk_limit = dev.spi_frame_limit().saturating_sub(3).max(1);
    let mut offset = 0usize;
    while offset < page_size {
        let chunk = (page_size - offset).min(chunk_limit);
        dev.cs_low()?;
        dev.spi_tx(&[0x03, ((offset >> 8) & 0xFF) as u8, (offset & 0xFF) as u8])?;
        dev.spi_rx(&mut buf[offset..offset + chunk])?;
        dev.cs_high()?;
        offset += chunk;
    }
    Ok(buf)
}

fn nand_load_page(dev: &Ch34xDevice, page_no: u32) -> Result<(), String> {
    nand_wait_ready(dev, 200)?;
    cs_cmd(
        dev,
        &[
            0x13,
            ((page_no >> 16) & 0xFF) as u8,
            ((page_no >> 8) & 0xFF) as u8,
            (page_no & 0xFF) as u8,
        ],
    )?;
    nand_wait_ready(dev, 200)
}

/// Read `len` bytes from the page cache at an arbitrary 16-bit column.
/// SPI NAND exposes main area first, then the spare/OOB area, so the spare
/// area starts at column `page_size`.
fn nand_read_cache(dev: &Ch34xDevice, column: usize, len: usize) -> Result<Vec<u8>, String> {
    let mut out = vec![0xFFu8; len];
    let chunk_limit = dev.spi_frame_limit().saturating_sub(4).max(1);
    let mut offset = 0usize;
    while offset < len {
        let chunk = (len - offset).min(chunk_limit);
        let col = column + offset;
        dev.cs_low()?;
        dev.spi_tx(&[0x03, ((col >> 8) & 0xFF) as u8, (col & 0xFF) as u8, 0x00])?;
        dev.spi_rx(&mut out[offset..offset + chunk])?;
        dev.cs_high()?;
        offset += chunk;
    }
    Ok(out)
}

/// Read the spare/OOB area of one NAND page.
pub fn nand_read_spare(
    dev: &Ch34xDevice,
    page_no: u32,
    page_size: usize,
    spare_size: usize,
) -> Result<Vec<u8>, String> {
    nand_load_page(dev, page_no)?;
    nand_read_cache(dev, page_size, spare_size)
}

/// Scan every block's first page spare area and report factory bad-block
/// markers. The standard marker is any value other than 0xFF at spare[0];
/// a fully erased 0xFF spare area means the block is good.
pub fn nand_scan_bad_blocks(
    dev: &Ch34xDevice,
    params: &ChipParams,
    size: u64,
    progress: &mut dyn FnMut(u32, u32),
) -> Result<Vec<u32>, String> {
    let page_size = params.page.max(1) as u64;
    let block_size = (params.block as u64).max(page_size);
    let pages_per_block = (block_size / page_size).max(1) as u32;
    let total_blocks = size.div_ceil(block_size).min(u32::MAX as u64) as u32;
    // IMSProg does not carry spare size for every NAND part; 64 bytes is the
    // most common default and the bad marker is at byte 0 either way.
    let spare_size = if params.spare > 0 { params.spare } else { 64 };

    let mut bad_blocks = Vec::new();
    for block_no in 0..total_blocks {
        let page_no = block_no * pages_per_block;
        let spare = nand_read_spare(dev, page_no, page_size as usize, spare_size)?;
        if spare.first().copied().unwrap_or(0xFF) != 0xFF {
            bad_blocks.push(block_no);
        }
        progress(block_no + 1, total_blocks);
    }
    Ok(bad_blocks)
}

/// Read the NAND unique ID (experimental, hardware validation pending).
/// Winbond-family SPI NAND: 4Bh + four dummy bytes + 64 data bytes.
pub fn nand_read_uid(dev: &Ch34xDevice, len: usize) -> Result<Vec<u8>, String> {
    nand_wait_ready(dev, 200)?;
    let mut out = vec![0xFFu8; len];
    dev.cs_low()?;
    dev.spi_tx(&[0x4B])?;
    dev.spi_tx(&[0x00, 0x00, 0x00, 0x00])?;
    dev.spi_rx(&mut out)?;
    dev.cs_high()?;
    Ok(out)
}

/// Read one 256-byte ONFI parameter page (experimental, hardware validation
/// pending). GigaDevice / Winbond families expose it through ECh + address 00h
/// + one dummy byte.
pub fn nand_read_param_page(dev: &Ch34xDevice) -> Result<Vec<u8>, String> {
    nand_wait_ready(dev, 200)?;
    let mut out = vec![0xFFu8; 256];
    dev.cs_low()?;
    dev.spi_tx(&[0xEC, 0x00, 0x00])?;
    dev.spi_rx(&mut out)?;
    dev.cs_high()?;
    Ok(out)
}

/// One BBM LUT link. LBA[15:14] encode status: 00 free, 10 valid, 11 invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BbmLutEntry {
    pub index: u8,
    pub lba: u16,
    pub pba: u16,
    pub free: bool,
    pub valid: bool,
}

fn parse_bbm_lut(raw: &[u8]) -> Vec<BbmLutEntry> {
    let mut entries = Vec::new();
    for (index, chunk) in raw.chunks_exact(4).enumerate().take(20) {
        let lba = u16::from_le_bytes([chunk[0], chunk[1]]);
        let pba = u16::from_le_bytes([chunk[2], chunk[3]]);
        let status = lba >> 14;
        entries.push(BbmLutEntry {
            index: index as u8,
            lba,
            pba,
            free: status == 0b00,
            valid: status == 0b10,
        });
    }
    entries
}

/// Read the 80-byte internal BBM LUT through the A5h command
/// (experimental, hardware validation pending). Each link is 4 bytes:
/// LBA[15:0] then PBA[15:0].
pub fn nand_read_bbm_lut(dev: &Ch34xDevice) -> Result<(Vec<BbmLutEntry>, Vec<u8>), String> {
    nand_wait_ready(dev, 200)?;
    let mut raw = vec![0xFFu8; 80];
    dev.cs_low()?;
    dev.spi_tx(&[0xA5])?;
    dev.spi_rx(&mut raw)?;
    dev.cs_high()?;
    Ok((parse_bbm_lut(&raw), raw))
}

/// Create one LBA->PBA block swap with the A1h command. Write enable must
/// be issued first, as required by Winbond-family SPI NAND.
pub fn nand_write_bbm_swap(dev: &Ch34xDevice, lba: u16, pba: u16) -> Result<(), String> {
    nand_wait_ready(dev, 200)?;
    nand_write_enable(dev)?;
    let lba_word = 0x8000 | lba; // LBA[15:14] = 10: valid link
    dev.cs_low()?;
    dev.spi_tx(&[
        0xA1,
        (lba_word & 0xFF) as u8,
        ((lba_word >> 8) & 0xFF) as u8,
        (pba & 0xFF) as u8,
        ((pba >> 8) & 0xFF) as u8,
    ])?;
    dev.cs_high()?;
    nand_wait_ready(dev, 500)
}

/// Prepare Bypass mode: scan results are written into the chip's internal
/// BBM LUT so hardware remaps bad logical blocks to reserved spare blocks.
/// Returns the newly created links.
pub fn nand_prepare_bypass_lut(
    dev: &Ch34xDevice,
    params: &ChipParams,
    size: u64,
    bad_blocks: &[u32],
) -> Result<Vec<(u16, u16)>, String> {
    let block_size = (params.block as u64).max(params.page.max(1) as u64);
    let total_blocks = size.div_ceil(block_size).min(u16::MAX as u64) as u16;
    let reserved = (total_blocks as u32 / 50).clamp(1, 20) as u16;
    let spare_start = total_blocks.saturating_sub(reserved);

    let mut lut = nand_read_bbm_lut(dev)?.0;
    let mut used_pbas: std::collections::HashSet<u16> =
        lut.iter().filter(|e| e.valid).map(|e| e.pba).collect();
    let valid_count = lut.iter().filter(|e| e.valid).count();
    let mut new_links = Vec::new();

    for block in bad_blocks {
        let lba = (*block).min(u16::MAX as u32) as u16;
        if lut.iter().any(|e| e.valid && (e.lba & 0x3FFF) == lba) {
            continue; // already mapped
        }
        if valid_count + new_links.len() >= 20 {
            return Err("BBM LUT 已满（最多 20 条映射）".into());
        }
        let spare = (spare_start..total_blocks).find(|pba| {
            let pba = *pba;
            !bad_blocks.contains(&(pba as u32))
                && !used_pbas.contains(&pba)
                && !new_links.iter().any(|(_, used)| *used == pba)
        });
        let Some(spare) = spare else {
            return Err(format!(
                "没有可用的备用块用于坏块 0x{:X}（备用区从块 {} 开始）",
                lba, spare_start
            ));
        };
        nand_write_bbm_swap(dev, lba, spare)?;
        used_pbas.insert(spare);
        new_links.push((lba, spare));
        lut.push(BbmLutEntry {
            index: 0,
            lba: 0x8000 | lba,
            pba: spare,
            free: false,
            valid: true,
        });
    }
    Ok(new_links)
}

/// Read configuration register B0h through Get Feature (0Fh).
pub fn nand_get_ecc(dev: &Ch34xDevice) -> Result<bool, String> {
    nand_wait_ready(dev, 200)?;
    let mut cfg = [0xFFu8; 1];
    dev.cs_low()?;
    dev.spi_tx(&[0x0F, 0xB0])?;
    dev.spi_rx(&mut cfg)?;
    dev.cs_high()?;
    Ok((cfg[0] & 0x10) != 0)
}

/// Toggle the on-die ECC bit (bit 4 of configuration register B0h) through
/// Set Feature (1Fh). Follows the Linux spi-nand CFG_ECC_ENABLE convention.
pub fn nand_set_ecc(dev: &Ch34xDevice, enable: bool) -> Result<(), String> {
    nand_wait_ready(dev, 200)?;
    let mut cfg = [0xFFu8; 1];
    dev.cs_low()?;
    dev.spi_tx(&[0x0F, 0xB0])?;
    dev.spi_rx(&mut cfg)?;
    dev.cs_high()?;

    if enable {
        cfg[0] |= 0x10;
    } else {
        cfg[0] &= !0x10;
    }
    dev.cs_low()?;
    dev.spi_tx(&[0x1F, 0xB0, cfg[0]])?;
    dev.cs_high()?;
    nand_wait_ready(dev, 200)
}

fn nand_page_write(dev: &Ch34xDevice, page: &[u8], page_no: u32) -> Result<(), String> {
    if 3 + page.len() > dev.spi_frame_limit() {
        return Err(format!(
            "SPI_PAGE_TOO_LARGE: 当前 HAL 单帧上限 {} 字节，NAND 页 {} 字节放不下。\
             建议改用 CH347 或 libusb 后端（二者都能正常写大页）。\
             若仍要用当前后端强制尝试分段写入，请确认后重试。",
            dev.spi_frame_limit(),
            page.len()
        ));
    }
    nand_wait_ready(dev, 1000)?;
    nand_write_enable(dev)?;
    dev.cs_low()?;
    dev.spi_tx(&[0x02, 0x00, 0x00])?;
    dev.spi_tx(page)?;
    dev.cs_high()?;
    nand_wait_ready(dev, 500)?;

    nand_write_enable(dev)?;
    cs_cmd(
        dev,
        &[
            0x10,
            ((page_no >> 16) & 0xFF) as u8,
            ((page_no >> 8) & 0xFF) as u8,
            (page_no & 0xFF) as u8,
        ],
    )?;
    nand_wait_ready(dev, 1000)
}

/// 单帧放不下整页时的“强制尝试”：把 0x02 缓冲区加载拆成多个列地址段，
/// 最后只执行一次 0x10。注意：该路径尚未经过真机验证，属于用户主动选择。
fn nand_page_write_segmented(dev: &Ch34xDevice, page: &[u8], page_no: u32) -> Result<(), String> {
    nand_wait_ready(dev, 1000)?;
    let chunk_limit = dev.spi_frame_limit().saturating_sub(3).max(1);
    let mut offset = 0usize;
    while offset < page.len() {
        let chunk = (page.len() - offset).min(chunk_limit);
        dev.cs_low()?;
        dev.spi_tx(&[0x02, ((offset >> 8) & 0xFF) as u8, (offset & 0xFF) as u8])?;
        dev.spi_tx(&page[offset..offset + chunk])?;
        dev.cs_high()?;
        offset += chunk;
    }
    nand_wait_ready(dev, 500)?;
    nand_write_enable(dev)?;
    cs_cmd(
        dev,
        &[
            0x10,
            ((page_no >> 16) & 0xFF) as u8,
            ((page_no >> 8) & 0xFF) as u8,
            (page_no & 0xFF) as u8,
        ],
    )?;
    nand_wait_ready(dev, 1000)
}

fn nand_unprotect(dev: &Ch34xDevice) -> Result<(), String> {
    nand_write_enable(dev)?;
    dev.cs_low()?;
    dev.spi_tx(&[0x0F, 0xA0])?;
    let mut prot = [0u8; 1];
    dev.spi_rx(&mut prot)?;
    dev.cs_high()?;
    dev.cs_low()?;
    dev.spi_tx(&[0x1F, 0xA0, prot[0] & 0x83])?;
    dev.cs_high()?;
    Ok(())
}

fn nand_block_erase(dev: &Ch34xDevice, block_no: u32) -> Result<(), String> {
    nand_wait_ready(dev, 2000)?;
    let row = block_no << 6; // IMSProg: PA[15:6], fixed 64 pages/block
    nand_write_enable(dev)?;
    nand_unprotect(dev)?;
    cs_cmd(
        dev,
        &[
            0xD8,
            ((row >> 16) & 0xFF) as u8,
            ((row >> 8) & 0xFF) as u8,
            (row & 0xFF) as u8,
        ],
    )?;
    nand_wait_ready(dev, 2000)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NandBadBlockMode {
    Skip,
    Bypass,
    Ignore,
}

pub fn nand_read(
    dev: &Ch34xDevice,
    params: &ChipParams,
    size: u64,
    bad_blocks: &[u32],
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let page_size = params.page.max(1) as u64;
    let block_size = (params.block as u64).max(page_size);
    let pages_per_block = (block_size / page_size).max(1) as u32;
    let mut bad_set: std::collections::HashSet<u32> = bad_blocks.iter().copied().collect();
    let mut out = Vec::with_capacity(size as usize);

    if bad_set.is_empty() {
        bad_set = std::collections::HashSet::new();
    }

    let total_blocks = size.div_ceil(block_size);
    for block_no in 0..total_blocks {
        if bad_set.contains(&(block_no as u32)) {
            let fill = block_size.min(size - out.len() as u64) as usize;
            out.resize(out.len() + fill, 0xFF);
            progress(out.len() as u64, size);
            continue;
        }
        for page_in_block in 0..pages_per_block {
            let page_no = block_no as u32 * pages_per_block + page_in_block;
            let mut page = nand_page_read(dev, params.page.max(1), page_no)?;
            if page.len() > size as usize - out.len() {
                page.truncate(size as usize - out.len());
            }
            out.extend_from_slice(&page);
            progress(out.len() as u64, size);
            if out.len() as u64 >= size {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)] // caller-owned NAND write flags, grouped later with BBM work
pub fn nand_write(
    dev: &Ch34xDevice,
    params: &ChipParams,
    data: &[u8],
    size: u64,
    bad_blocks: &[u32],
    mode: NandBadBlockMode,
    force_segmented: bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    if mode == NandBadBlockMode::Bypass && !bad_blocks.is_empty() {
        return Err(
            "Bypass(BBM LUT) 写入将在坏块映射表功能完成后启用，请暂时使用 Skip 或 Ignore".into(),
        );
    }
    let page_size = params.page.max(1) as u64;
    let block_size = (params.block as u64).max(page_size);
    let pages_per_block = (block_size / page_size).max(1) as u32;
    let bad_set: std::collections::HashSet<u32> = bad_blocks.iter().copied().collect();
    let total = data.len() as u64;
    let total_pages = size.div_ceil(page_size);

    let mut offset = 0usize;
    let mut page_no = 0u32;
    while offset < data.len() {
        let block_no = page_no / pages_per_block;
        if bad_set.contains(&block_no) {
            // Skip the whole physical block; logical data advances unchanged.
            page_no += pages_per_block;
            continue;
        }
        if page_no as u64 >= total_pages {
            return Err("写入数据超过 NAND 可用物理容量（Skip 模式会跳过坏块）".into());
        }
        let chunk = (data.len() - offset).min(params.page.max(1));
        let mut page = vec![0xFFu8; params.page.max(1)];
        page[..chunk].copy_from_slice(&data[offset..offset + chunk]);
        if 3 + page.len() > dev.spi_frame_limit() {
            if force_segmented {
                nand_page_write_segmented(dev, &page, page_no)?;
            } else {
                return Err(format!(
                    "SPI_PAGE_TOO_LARGE: 当前 HAL 单帧上限 {} 字节，NAND 页 {} 字节放不下。\
                     建议改用 CH347 或 libusb 后端（二者都能正常写大页）。\
                     若仍要用当前后端强制尝试分段写入，请确认后重试。",
                    dev.spi_frame_limit(),
                    page.len()
                ));
            }
        } else {
            nand_page_write(dev, &page, page_no)?;
        }
        offset += chunk;
        page_no += 1;
        progress(offset as u64, total);
    }
    Ok(())
}

pub fn nand_erase(
    dev: &Ch34xDevice,
    params: &ChipParams,
    size: u64,
    bad_blocks: &[u32],
) -> Result<(), String> {
    let block_size = params.block.max(1) as u64;
    let blocks = size.div_ceil(block_size) as u32;
    let bad_set: std::collections::HashSet<u32> = bad_blocks.iter().copied().collect();
    for block_no in 0..blocks {
        if bad_set.contains(&block_no) {
            continue;
        }
        nand_block_erase(dev, block_no)?;
    }
    Ok(())
}

// ═════════════════════════════ I2C EEPROM (24xx) ═════════════════════════════

fn i2c_device_address(address: u32, algorithm: u8) -> u8 {
    let mask = (algorithm & 0xF0) >> 4;
    if (algorithm & 0x0F) == 0x01 {
        ((((address & 0xFF00) >> 8) as u8) & mask) << 1 | 0xA0
    } else {
        ((((address & 0xFF0000) >> 16) as u8) & mask) << 1 | 0xA0
    }
}

pub fn i2c_read(
    dev: &Ch34xDevice,
    params: &ChipParams,
    start_addr: u64,
    len: usize,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let algorithm = params.algorithm;
    // IMSProg read chunk: 16 bytes CH341, 64 bytes CH347
    let max_chunk = if dev.is_ch347() { 64 } else { 16 };
    let total = len as u64;
    let mut out = Vec::with_capacity(len);
    let mut addr = start_addr;
    let mut done = 0usize;

    while done < len {
        let chunk = (len - done).min(max_chunk);
        let mut pkt: Vec<u8> = vec![0xAA, 0x74]; // I2C_STREAM, STA
        let dev_addr = i2c_device_address(addr as u32, algorithm);
        if (algorithm & 0x0F) == 0x01 {
            pkt.push(0x80 | 2);
            pkt.push(dev_addr);
            pkt.push((addr & 0xFF) as u8);
        } else if (algorithm & 0x0F) == 0x02 {
            pkt.push(0x80 | 3);
            pkt.push(dev_addr);
            pkt.push(((addr >> 8) & 0xFF) as u8);
            pkt.push((addr & 0xFF) as u8);
        } else {
            return Err(format!("不支持的 I2C 地址算法 0x{:02X}", algorithm));
        }
        pkt.push(0x74); // repeated START
        pkt.push(0x80 | 1);
        pkt.push(dev_addr | 0x01);
        pkt.push(0xC0 | ((chunk - 1) as u8)); // IN N-1 with ACK
        pkt.push(0xC0); // last byte, NACK
        pkt.push(0x75); // STOP
        pkt.push(0x00); // END

        dev.i2c_write(&pkt)?;
        let mut raw = vec![0u8; chunk + 4];
        let got = dev.i2c_read(&mut raw)?;
        let offset = if dev.is_ch347() {
            if (algorithm & 0x0F) == 0x02 {
                4
            } else {
                3
            }
        } else {
            0
        };
        if got < offset + chunk {
            return Err(format!(
                "I2C 读取长度不符: 收到 {} 字节, 需要 {} 字节",
                got,
                offset + chunk
            ));
        }
        out.extend_from_slice(&raw[offset..offset + chunk]);

        addr += chunk as u64;
        done += chunk;
        progress(done as u64, total);
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(out)
}

pub fn i2c_write(
    dev: &Ch34xDevice,
    params: &ChipParams,
    data: &[u8],
    start_addr: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let algorithm = params.algorithm;
    let page_size = params.sector.max(1);
    let total = data.len() as u64;
    let mut offset = 0usize;
    let mut addr = start_addr;

    while offset < data.len() {
        let mut chunk = (data.len() - offset).min(page_size);
        if !dev.is_ch347() {
            chunk = chunk.min(16); // CH341 bulk endpoint limit in IMSProg
        }
        let mut pkt: Vec<u8> = vec![0xAA, 0x74]; // I2C_STREAM, STA
        let dev_addr = i2c_device_address(addr as u32, algorithm);
        if (algorithm & 0x0F) == 0x01 {
            pkt.push(0x80 | 2);
            pkt.push(dev_addr);
            pkt.push((addr & 0xFF) as u8);
        } else if (algorithm & 0x0F) == 0x02 {
            pkt.push(0x80 | 3);
            pkt.push(dev_addr);
            pkt.push(((addr >> 8) & 0xFF) as u8);
            pkt.push((addr & 0xFF) as u8);
        } else {
            return Err(format!("不支持的 I2C 地址算法 0x{:02X}", algorithm));
        }
        pkt.push(0x80 | (chunk as u8));
        pkt.extend_from_slice(&data[offset..offset + chunk]);
        pkt.push(0x75); // STOP
        pkt.push(0x00); // END

        dev.i2c_write(&pkt)?;
        if dev.is_ch347() {
            // CH347 firmware echoes ACK bytes; IMPROG reads 512 and requires 0x01.
            let mut ack = vec![0u8; 512];
            let got = dev.i2c_read(&mut ack)?;
            for &b in &ack[..got] {
                if b != 0x01 {
                    return Err("I2C 写入 NACK".into());
                }
            }
        }

        addr += chunk as u64;
        offset += chunk;
        progress(offset as u64, total);
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

pub fn i2c_erase(dev: &Ch34xDevice, params: &ChipParams, size: u64) -> Result<(), String> {
    // I2C EEPROM "erase" == write 0xFF to every cell.
    let erased = vec![0xFFu8; size as usize];
    let mut progress = |_done: u64, _total: u64| {};
    i2c_write(dev, params, &erased, 0, &mut progress)
}

// ═════════════════════════════ Microwire 93xx (CH341 only) ═══════════════════

const MW_CLK: u8 = 1 << 3;
const MW_DO: u8 = 1 << 7;
const MW_DI: u8 = 1 << 5;
const MW_CS: u8 = 1 << 0;

struct Mw<'a> {
    dev: &'a Ch34xDevice,
    data: u8,
}

impl<'a> Mw<'a> {
    fn new(dev: &'a Ch34xDevice) -> Self {
        Mw { dev, data: 0 }
    }

    fn sync(&mut self) -> Result<(), String> {
        self.dev.gpio_setbits(self.data)
    }

    fn set(&mut self, mask: u8, val: bool) -> Result<(), String> {
        if val {
            self.data |= mask;
        } else {
            self.data &= !mask;
        }
        self.sync()
    }

    fn clock(&mut self, val: bool) -> Result<(), String> {
        self.set(MW_CLK, val)
    }

    fn cs(&mut self, val: bool) -> Result<(), String> {
        self.set(MW_CS, val)
    }

    fn di(&mut self, val: bool) -> Result<(), String> {
        self.set(MW_DI, val)
    }

    fn get_do(&mut self) -> Result<bool, String> {
        Ok((self.dev.gpio_getbits()? & MW_DO) != 0)
    }

    fn send(&mut self, val: u32, nbit: u32) -> Result<(), String> {
        for i in 0..nbit {
            let bit = (val & (1 << (nbit - i - 1))) != 0;
            self.clock(false)?;
            self.di(bit)?;
            std::thread::sleep(Duration::from_millis(1));
            self.clock(true)?;
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    fn get_byte(&mut self) -> Result<u8, String> {
        let mut val = 0u8;
        for i in 0..8 {
            self.clock(false)?;
            std::thread::sleep(Duration::from_millis(1));
            let bit = self.get_do()?;
            self.clock(true)?;
            std::thread::sleep(Duration::from_millis(1));
            val |= (bit as u8) << (7 - i);
        }
        Ok(val)
    }

    fn wait_busy(&mut self) -> Result<(), String> {
        for _ in 0..10_000 {
            self.clock(false)?;
            std::thread::sleep(Duration::from_millis(1));
            let busy = !self.get_do()?;
            self.clock(true)?;
            std::thread::sleep(Duration::from_millis(1));
            if !busy {
                return Ok(());
            }
        }
        Err("Microwire 芯片一直忙".into())
    }

    fn write_enable(&mut self, num_bit: u32) -> Result<(), String> {
        self.cs(false)?;
        self.clock(false)?;
        self.di(true)?;
        std::thread::sleep(Duration::from_millis(1));
        self.cs(true)?;
        std::thread::sleep(Duration::from_millis(1));
        self.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));
        self.send(3, 4)?;
        self.send(0, num_bit - 2)?;
        self.di(false)?;
        std::thread::sleep(Duration::from_millis(1));
        self.cs(false)?;
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }

    fn write_disable(&mut self, num_bit: u32) -> Result<(), String> {
        self.cs(true)?;
        std::thread::sleep(Duration::from_millis(1));
        self.clock(false)?;
        std::thread::sleep(Duration::from_millis(1));
        self.di(true)?;
        std::thread::sleep(Duration::from_millis(1));
        self.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));
        self.send(0, 4)?;
        self.send(0, num_bit - 2)?;
        self.cs(false)?;
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
}

fn mw_org_and_bits(algorithm: u8) -> (bool, u32) {
    let org = (algorithm & 0xF0) != 0;
    let mut num_bit = (algorithm & 0x0F) as u32;
    if org {
        num_bit -= 1;
    }
    (org, num_bit)
}

pub fn mw_read(
    dev: &Ch34xDevice,
    params: &ChipParams,
    start_addr: u64,
    len: usize,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let (org, num_bit) = mw_org_and_bits(params.algorithm);
    if num_bit < 2 {
        return Err("Microwire 地址位数无效".into());
    }
    let total = len as u64;
    let mut mw = Mw::new(dev);
    let mut out = Vec::with_capacity(len);

    let base = if org {
        (start_addr as u32) / 2
    } else {
        start_addr as u32
    };
    let words = if org { len / 2 } else { len };

    for w in 0..words {
        let byte_addr = base + w as u32;
        mw.cs(false)?;
        mw.clock(false)?;
        mw.di(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.cs(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));

        mw.send(2, 2)?;
        mw.send(byte_addr, num_bit)?;
        mw.di(false)?;
        mw.clock(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));

        let b1 = mw.get_byte()?;
        if org {
            let b2 = mw.get_byte()?;
            if out.len() < len {
                out.push(b2);
            }
            if out.len() < len {
                out.push(b1);
            }
        } else {
            out.push(b1);
        }

        mw.cs(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));
        progress(out.len() as u64, total);
    }
    Ok(out)
}

pub fn mw_write(
    dev: &Ch34xDevice,
    params: &ChipParams,
    data: &[u8],
    start_addr: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let (org, num_bit) = mw_org_and_bits(params.algorithm);
    if num_bit < 2 {
        return Err("Microwire 地址位数无效".into());
    }
    let total = data.len() as u64;
    let mut mw = Mw::new(dev);
    mw.write_enable(num_bit)?;

    let base = if org {
        (start_addr as u32) / 2
    } else {
        start_addr as u32
    };
    let mut address = base;
    let mut l = 0usize;
    while l < data.len() {
        mw.cs(false)?;
        mw.clock(false)?;
        mw.di(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.cs(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.send(1, 2)?;
        mw.send(address, num_bit)?;

        if org {
            let b1 = if l + 1 < data.len() {
                data[l + 1]
            } else {
                0xFF
            };
            let b0 = data[l];
            mw.send(b1 as u32, 8)?;
            mw.send(b0 as u32, 8)?;
            l += 2;
        } else {
            mw.send(data[l] as u32, 8)?;
            l += 1;
        }
        mw.clock(false)?;
        mw.di(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.cs(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.cs(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.wait_busy()?;
        mw.cs(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(false)?;
        std::thread::sleep(Duration::from_millis(1));
        mw.clock(true)?;
        std::thread::sleep(Duration::from_millis(1));

        address += 1;
        progress(l as u64, total);
    }
    mw.write_disable(num_bit)
}

pub fn mw_erase(dev: &Ch34xDevice, params: &ChipParams) -> Result<(), String> {
    let (_org, num_bit) = mw_org_and_bits(params.algorithm);
    if num_bit < 3 {
        return Err("Microwire 地址位数无效".into());
    }
    let mut mw = Mw::new(dev);
    mw.write_enable(num_bit)?;

    mw.cs(false)?;
    mw.clock(false)?;
    mw.di(true)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.cs(true)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.clock(true)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.send(2, 4)?;
    mw.send(0, num_bit - 3)?;
    mw.clock(false)?;
    mw.di(false)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.cs(false)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.cs(true)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.clock(true)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.wait_busy()?;
    mw.cs(false)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.clock(false)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.clock(true)?;
    std::thread::sleep(Duration::from_millis(1));
    mw.write_disable(num_bit)
}
