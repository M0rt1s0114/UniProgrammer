// Shared application state and device-level operations that do not depend on
// the UI transport. The Tauri command layer stays a thin adapter on top of
// these functions so the state can be driven from any frontend later.

use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ch34x::{Ch34xDevice, DeviceMode};
use crate::{chiplib, serprog, sfdp};

#[derive(Serialize)]
pub struct ChipDetectResult {
    pub text: String,
    pub info: Option<ChipDetectInfo>,
}

#[derive(Serialize)]
pub struct ChipDetectInfo {
    pub id: String,
    pub vendor: String,
    pub model: String,
    pub protocol: String,
    pub size: u64,
    pub page: u32,
    pub sector: Option<u64>,
    pub block: Option<u64>,
    pub addr4bit: Option<u32>,
    pub vcc: Option<String>,
    pub spare: Option<u64>,
    #[serde(rename = "pagesPerBlock")]
    pub pages_per_block: Option<u32>,
    #[serde(rename = "isBmm")]
    pub is_bmm: Option<bool>,
    #[serde(rename = "dummyMode")]
    pub dummy_mode: Option<String>,
    #[serde(rename = "readMode")]
    pub read_mode: Option<String>,
    #[serde(rename = "writeMode")]
    pub write_mode: Option<String>,
    pub feature: Option<u32>,
}

pub struct AppState {
    pub ch34x: Option<crate::ch34x::Ch34xSettings>,
    pub serprog: Option<serprog::Serprog>,
    pub lib: Option<chiplib::Chiplib>,
    pub connected_device: Option<String>,
    pub detected: Option<chiplib::ChipInfo>,
    /// Last serial-port snapshot. Serial probing only runs again when this
    /// list changes, so the hotplug poll never chats with every COM port.
    pub last_serial_ports: Vec<String>,
    /// Last serprog probe result, reused while the port list is unchanged.
    pub cached_serprog: Vec<crate::autodetect::ProgrammerCandidate>,
    /// Mirrored from the frontend: true while read/write/erase/verify/auto
    /// flow is executing. Used by the Rust close-requested handler.
    pub operation_running: bool,
}

pub fn format_human_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    let (unit, label) = if size >= GB {
        (GB, "GB")
    } else if size >= MB {
        (MB, "MB")
    } else if size >= KB {
        (KB, "KB")
    } else {
        return format!("{} B", size);
    };

    if size.is_multiple_of(unit) {
        format!("{} {}", size / unit, label)
    } else {
        format!("{:.1} {}", size as f64 / unit as f64, label)
    }
}

pub fn get_lib(state: &AppState) -> Result<&chiplib::Chiplib, String> {
    state.lib.as_ref().ok_or("芯片库未加载".into())
}

/// Open the selected programmer. The returned handle owns the USB device and
/// closes it on drop (per-operation lifecycle, same as IMSProg).
pub fn open_ch34x(state: &AppState) -> Result<Ch34xDevice, String> {
    open_ch34x_mode(state, DeviceMode::Spi)
}

pub fn open_ch34x_mode(state: &AppState, mode: DeviceMode) -> Result<Ch34xDevice, String> {
    let settings = state
        .ch34x
        .as_ref()
        .ok_or("没有可用的 CH34X 编程器，请先连接")?;
    Ch34xDevice::open_with_mode(settings, mode)
}

/// IMSProg snor_read_devid(): JEDEC ID, 5 bytes like IMSProg.
pub fn spi_read_jedec(dev: &Ch34xDevice) -> Result<[u8; 5], String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x9F])?;
    let mut id = [0xFFu8; 5];
    dev.spi_rx(&mut id)?;
    dev.cs_high()?;
    Ok(id)
}

/// SPI NAND read-ID variant: 9Fh + manufacturer-ID address byte 00h.
/// Some NAND vendors only output the ID after receiving the address byte;
/// the plain 9Fh probe then returns a leading FF byte.
pub fn spi_read_jedec_with_addr(dev: &Ch34xDevice) -> Result<[u8; 5], String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x9F, 0x00])?;
    let mut id = [0xFFu8; 5];
    dev.spi_rx(&mut id)?;
    dev.cs_high()?;
    Ok(id)
}

// ═══════════════════════════════ CH34X SPI helpers (IMPROG style) ═════════════════════════

/// 3-byte addressing covers exactly 16 MiB (addresses 0x000000..0xFFFFFF).
/// 4-byte mode is only required above 16 MiB; 16 MiB chips such as
/// EN25QH128 must stay in 3-byte mode.
pub fn nor_requires_4byte(size: u64) -> bool {
    size > 0x0100_0000
}

/// NOR parameters used by the read/write/erase paths. Missing fields fall back
/// to safe defaults until the chip database is enriched with IMSProg fields.
pub struct NorParams {
    pub page: usize,
    pub _sector: usize,
    pub _block: usize,
    pub addr4b: bool,
    pub alg: u8,
}

pub fn nor_params(info: &chiplib::ChipInfo) -> NorParams {
    let page = info.page.max(1) as usize;
    // attr names follow IMSProg database semantics after the importer runs:
    // addr4bit: low nibble 1 = use 4-byte addressing, high nibble = algorithm
    // (0 default B7/E9, 1 Winbond, 2 Spansion BRWR).
    let addr4bit = info.attr_u32("addr4bit").unwrap_or(0) as u8;
    let addr4b = (addr4bit & 0x0F) != 0 || nor_requires_4byte(info.size);
    let alg = (addr4bit >> 4) & 0x0F;
    NorParams {
        page,
        _sector: info.attr_u32("sector").unwrap_or(4096) as usize,
        _block: info.attr_u32("block").unwrap_or(64 * 1024) as usize,
        addr4b,
        alg,
    }
}

pub fn spi_read_status_register(dev: &Ch34xDevice, opcode: u8) -> Result<u8, String> {
    dev.cs_low()?;
    dev.spi_tx(&[opcode])?;
    let mut status = [0xFFu8; 1];
    dev.spi_rx(&mut status)?;
    dev.cs_high()?;
    Ok(status[0])
}

pub fn spi_read_status(dev: &Ch34xDevice) -> Result<u8, String> {
    spi_read_status_register(dev, 0x05)
}

/// SR1 BP0..BP2. A few parts keep BP4 in SR2; callers can add it when SR2
/// actually answered (all-0xFF usually means the register does not exist).
pub const NOR_BP_MASK_SR1: u8 = 0x04 | 0x08 | 0x10;

#[derive(Clone, Serialize)]
pub struct NorWriteProtectStatus {
    sr1: u8,
    sr2: u8,
    sr3: u8,
    /// BP0..BP2 (SR1 bits 2-4) plus common BP4 (SR2 bit 6) when SR2 is valid.
    bp_bits: u8,
    write_protected: bool,
}

/// Raw status-register snapshot used by the write-protect commands.
pub fn nor_wp_snapshot(dev: &Ch34xDevice) -> Result<NorWriteProtectStatus, String> {
    let sr1 = spi_read_status_register(dev, 0x05)?;
    let sr2 = spi_read_status_register(dev, 0x35).unwrap_or(0xFF);
    let sr3 = spi_read_status_register(dev, 0x15).unwrap_or(0xFF);
    let mut bp_bits = sr1 & NOR_BP_MASK_SR1;
    if sr2 != 0xFF && (sr2 & 0x40) != 0 {
        bp_bits |= 0x20;
    }
    Ok(NorWriteProtectStatus {
        sr1,
        sr2,
        sr3,
        bp_bits,
        write_protected: bp_bits != 0,
    })
}

/// Clear block-protect bits in SR1. Kept separate from `spi_unprotect` so the
/// erase path can keep its IMSProg-compatible behavior while the explicit
/// "disable write protect" command reports what it changed.
pub fn nor_clear_block_protect(dev: &Ch34xDevice) -> Result<NorWriteProtectStatus, String> {
    let before = nor_wp_snapshot(dev)?;
    if before.bp_bits == 0 {
        return Ok(before);
    }
    spi_wait_ready(dev, 1000)?;
    // EWSR for legacy parts that predate WREN-gated WRSR; WREN covers modern
    // parts. Both are harmless on the other family.
    dev.cs_low()?;
    dev.spi_tx(&[0x50])?;
    dev.cs_high()?;
    spi_write_enable(dev)?;
    let sr1_new = before.sr1 & !NOR_BP_MASK_SR1;
    dev.cs_low()?;
    dev.spi_tx(&[0x01, sr1_new])?;
    dev.cs_high()?;
    spi_wait_ready(dev, 2000)?;
    nor_wp_snapshot(dev)
}

/// IMSProg snor_wait_ready(): poll RDSR until WIP|EPE|WEL are all clear.
pub fn spi_wait_ready(dev: &Ch34xDevice, timeout_ms: u64) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let status = spi_read_status(dev)?;
        if (status & 0x01 | status & 0x20 | status & 0x02) == 0 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("等待闪存就绪超时".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn spi_write_enable(dev: &Ch34xDevice) -> Result<(), String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x06])?;
    dev.cs_high()
}

pub fn spi_write_disable(dev: &Ch34xDevice) -> Result<(), String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x04])?;
    dev.cs_high()
}

/// IMSProg snor_unprotect(): clear BP0-BP2 when they are set.
pub fn spi_unprotect(dev: &Ch34xDevice) -> Result<(), String> {
    let sr = spi_read_status(dev)?;
    if (sr & (0x04 | 0x08 | 0x10)) != 0 {
        spi_write_enable(dev)?;
        dev.cs_low()?;
        dev.spi_tx(&[0x01, 0x00])?;
        dev.cs_high()?;
        spi_wait_ready(dev, 1000)?;
    }
    Ok(())
}

/// IMSProg snor_4byte_mode(): B7/E9 (default), Winbond exit fix-up,
/// Spansion BRWR/BWRR path.
pub fn spi_4byte_mode(dev: &Ch34xDevice, alg: u8, enable: bool) -> Result<(), String> {
    spi_wait_ready(dev, 1000)?;
    if alg == 0x02 {
        // Spansion: write BRWR (0x17) and verify BRRD (0x16)
        let br = if enable { 0x81u8 } else { 0x00 };
        dev.cs_low()?;
        dev.spi_tx(&[0x17, br])?;
        dev.cs_high()?;
        dev.cs_low()?;
        dev.spi_tx(&[0x16])?;
        let mut readback = [0u8; 1];
        dev.spi_rx(&mut readback)?;
        dev.cs_high()?;
        if readback[0] != br {
            return Err(format!(
                "4B 模式切换失败 {}: 写入 0x{:02X}, 读回 0x{:02X}",
                if enable { "使能" } else { "退出" },
                br,
                readback[0]
            ));
        }
    } else {
        let cmd: u8 = if enable { 0xB7 } else { 0xE9 };
        dev.cs_low()?;
        dev.spi_tx(&[cmd])?;
        dev.cs_high()?;
        if !enable && alg == 0x01 {
            // Winbond: after exiting 4B mode, clear the extended register
            spi_write_enable(dev)?;
            dev.cs_low()?;
            dev.spi_tx(&[0xC5, 0x00])?;
            dev.cs_high()?;
        }
    }
    Ok(())
}

/// Read SPI NOR status registers (SR1/SR2/SR3) over serprog. Registers that
/// the chip does not implement return 0xFF and are treated as absent.
pub fn serprog_nor_wp_snapshot(
    ser: &mut serprog::Serprog,
) -> Result<NorWriteProtectStatus, String> {
    let sr1 = ser.spi_command(&[0x05], 1)?[0];
    let sr2 = ser.spi_command(&[0x35], 1).map(|v| v[0]).unwrap_or(0xFF);
    let sr3 = ser.spi_command(&[0x15], 1).map(|v| v[0]).unwrap_or(0xFF);
    let mut bp_bits = sr1 & NOR_BP_MASK_SR1;
    if sr2 != 0xFF && (sr2 & 0x40) != 0 {
        bp_bits |= 0x20;
    }
    Ok(NorWriteProtectStatus {
        sr1,
        sr2,
        sr3,
        bp_bits,
        write_protected: bp_bits != 0,
    })
}

/// Poll serprog RDSR until WIP clears or the timeout expires.
pub fn serprog_wait_ready(ser: &mut serprog::Serprog, timeout_ms: u64) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let resp = ser.spi_command(&[0x05], 1)?;
        if resp.len() == 1 && (resp[0] & 0x01) == 0 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("等待闪存就绪超时".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Write-protect status for the currently detected SPI NOR. The caller owns
/// the state lock; this function only touches device and chip state.
pub fn nor_wp_status(state: &mut AppState) -> Result<NorWriteProtectStatus, String> {
    let info = state
        .detected
        .as_ref()
        .ok_or("请先检测或选择 SPI NOR 芯片")?
        .clone();
    if info.protocol != "SPI_NOR" {
        return Err("当前芯片不是 SPI NOR".into());
    }
    if state.ch34x.is_some() {
        let dev = open_ch34x_mode(state, DeviceMode::Spi)?;
        nor_wp_snapshot(&dev)
    } else if let Some(ser) = state.serprog.as_mut() {
        serprog_nor_wp_snapshot(ser)
    } else {
        Err("没有可用的编程器".into())
    }
}

/// Disable SPI NOR block-protect bits and report the SR1 transition.
pub fn nor_wp_disable(state: &mut AppState) -> Result<String, String> {
    let info = state
        .detected
        .as_ref()
        .ok_or("请先检测或选择 SPI NOR 芯片")?
        .clone();
    if info.protocol != "SPI_NOR" {
        return Err("当前芯片不是 SPI NOR".into());
    }

    if state.ch34x.is_some() {
        let dev = open_ch34x_mode(state, DeviceMode::Spi)?;
        let before = nor_wp_snapshot(&dev)?;
        let after = nor_clear_block_protect(&dev)?;
        Ok(format!(
            "NOR 写保护已处理：SR1 0x{:02X} -> 0x{:02X}（保护位 {} -> {}）",
            before.sr1,
            after.sr1,
            if before.write_protected { "开" } else { "关" },
            if after.write_protected { "开" } else { "关" }
        ))
    } else if let Some(ser) = state.serprog.as_mut() {
        let before = serprog_nor_wp_snapshot(ser)?;
        if !before.write_protected {
            return Ok(format!(
                "NOR 写保护未开启（SR1=0x{:02X}），无需解除",
                before.sr1
            ));
        }
        let sr1_new = before.sr1 & !NOR_BP_MASK_SR1;
        ser.spi_command(&[0x50], 0)?; // EWSR (legacy parts)
        ser.spi_command(&[0x06], 0)?; // WREN (modern parts)
        ser.spi_command(&[0x01, sr1_new], 0)?; // WRSR
        serprog_wait_ready(ser, 2000)?;
        let after = serprog_nor_wp_snapshot(ser)?;
        Ok(format!(
            "NOR 写保护已解除：SR1 0x{:02X} -> 0x{:02X}",
            before.sr1, after.sr1
        ))
    } else {
        Err("没有可用的编程器".into())
    }
}

/// Build candidate ID strings from a raw ID probe.
///
/// The chip database contains both 6-hex JEDEC IDs (`C84015`) and legacy
/// 4-hex manufacturer+device IDs (`0125`). Some NAND devices shift the real
/// ID by one dummy byte, so the probe is matched both directly and with the
/// first byte skipped.
pub fn jedec_id_candidates(raw: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };

    if raw.len() >= 3 {
        push(format!("{:02X}{:02X}{:02X}", raw[0], raw[1], raw[2]));
    }
    if raw.len() >= 2 {
        push(format!("{:02X}{:02X}", raw[0], raw[1]));
    }
    if raw.len() >= 4 {
        push(format!("{:02X}{:02X}{:02X}", raw[1], raw[2], raw[3]));
    }
    if raw.len() >= 3 {
        push(format!("{:02X}{:02X}", raw[1], raw[2]));
    }
    out
}

/// SFDP read (JESD216): 0x5A + 3-byte SFDP-space address + one dummy byte.
fn sfdp_read_ch34x(dev: &Ch34xDevice, addr: u32, len: usize) -> Result<Vec<u8>, String> {
    let hdr = [
        0x5A,
        (addr >> 16) as u8,
        (addr >> 8) as u8,
        addr as u8,
        0x00, // dummy cycle
    ];
    if hdr.len() + len > dev.spi_frame_limit() {
        return Err("SFDP 读取长度超过当前 HAL 单帧上限".into());
    }
    dev.cs_low()?;
    dev.spi_tx(&hdr)?;
    let mut out = vec![0xFFu8; len];
    dev.spi_rx(&mut out)?;
    dev.cs_high()?;
    Ok(out)
}

fn sfdp_read_serprog(ser: &mut serprog::Serprog, addr: u32, len: usize) -> Result<Vec<u8>, String> {
    ser.spi_command(
        &[
            0x5A,
            (addr >> 16) as u8,
            (addr >> 8) as u8,
            addr as u8,
            0x00, // dummy cycle
        ],
        len,
    )
}

fn synthesize_sfdp_chip(jedec_id: &str, params: &sfdp::SfdpBasicFlashParams) -> chiplib::ChipInfo {
    let mut attrs = HashMap::new();
    attrs.insert("sector".to_string(), params.sector_size.to_string());
    attrs.insert("block".to_string(), params.block_size.to_string());
    attrs.insert("vcc".to_string(), "3.3".to_string());
    attrs.insert("sfdp".to_string(), "1".to_string());
    chiplib::ChipInfo {
        id: jedec_id.to_string(),
        vendor: "SFDP".to_string(),
        model: format!("Unknown {} (SFDP)", jedec_id.to_ascii_uppercase()),
        protocol: "SPI_NOR".to_string(),
        size: params.density_bytes,
        page: params.page_size,
        attrs,
    }
}

pub fn chip_info_to_detect(info: &chiplib::ChipInfo) -> ChipDetectInfo {
    ChipDetectInfo {
        id: info.id.clone(),
        vendor: info.vendor.clone(),
        model: info.model.clone(),
        protocol: info.protocol.clone(),
        size: info.size,
        page: info.page,
        sector: info.attr_u64("sector"),
        block: info.attr_u64("block"),
        addr4bit: info.attr_u32("addr4bit"),
        vcc: info.attr("vcc").map(|v| v.to_string()),
        spare: info.attr_u64("spare"),
        pages_per_block: info.attr_u32("pagesPerBlock").or_else(|| {
            info.attr_u64("block")
                .map(|block| (block / info.page.max(1) as u64).max(1) as u32)
        }),
        is_bmm: info
            .attr("IsBMM")
            .or_else(|| info.attr("isbmm"))
            .map(|v| v != "0"),
        dummy_mode: info.attr("dummyMode").map(|v| v.to_string()),
        read_mode: info.attr("readMode").map(|v| v.to_string()),
        write_mode: info.attr("writeMode").map(|v| v.to_string()),
        feature: info.attr_u32("feature"),
    }
}

/// Shared chip-detection pipeline: JEDEC lookup first, then SFDP fallback for
/// unlisted SPI NOR parts. No Tauri types or events are involved.
pub fn detect_chip(state: &mut AppState) -> Result<ChipDetectResult, String> {
    let mut probes: Vec<[u8; 5]> = Vec::new();

    if state.ch34x.is_some() {
        let dev = open_ch34x(state)?;
        probes.push(spi_read_jedec(&dev)?);
        probes.push(spi_read_jedec_with_addr(&dev)?);
    } else if let Some(ser) = &mut state.serprog {
        let first = ser.spi_command(&[0x9F], 3)?;
        let mut raw = [0xFFu8; 5];
        raw[0] = first[0];
        raw[1] = first[1];
        raw[2] = first[2];
        probes.push(raw);

        let second = ser.spi_command(&[0x9F, 0x00], 3)?;
        let mut raw2 = [0xFFu8; 5];
        raw2[0] = second[0];
        raw2[1] = second[1];
        raw2[2] = second[2];
        probes.push(raw2);
    } else {
        return Err("没有可用的编程器，请先初始化或连接 serprog".into());
    }

    let lib = get_lib(state)?;
    let mut matched: Option<chiplib::ChipInfo> = None;
    let mut matched_id = String::new();
    for probe in &probes {
        for candidate in jedec_id_candidates(probe) {
            if let Some(info) = lib.find_by_id(&candidate) {
                matched_id = candidate;
                matched = Some(info);
                break;
            }
        }
        if matched.is_some() {
            break;
        }
    }

    match matched {
        Some(info) => {
            let text = format!(
                "✅ 芯片匹配成功！\n厂商: {}\n型号: {}\n容量: {}\n页大小: {} 字节\n协议: {}\nJEDEC: {}\n（设备: {}）",
                info.vendor,
                info.model,
                format_human_size(info.size),
                info.page,
                info.protocol,
                matched_id,
                state.connected_device.as_deref().unwrap_or("未知")
            );
            let detected = info.clone();
            let result = ChipDetectResult {
                text,
                info: Some(chip_info_to_detect(&info)),
            };
            state.detected = Some(detected);
            Ok(result)
        }
        None => {
            // JEDEC ID is absent from chiplib. Before giving up, try JESD216
            // SFDP: an unlisted but SFDP-compliant NOR can be sized and used
            // from its Basic Flash Parameter Table.
            let first = probes[0];
            let jedec_id = format!("{:02X}{:02X}{:02X}", first[0], first[1], first[2]);
            let plausible_id = jedec_id != "FFFFFF" && jedec_id != "000000";
            let sfdp_match: Option<chiplib::ChipInfo> = if !plausible_id {
                None
            } else if state.ch34x.is_some() {
                let dev = open_ch34x(state)?;
                sfdp::discover_sfdp(|addr, len| sfdp_read_ch34x(&dev, addr, len))?
                    .map(|params| synthesize_sfdp_chip(&jedec_id, &params))
            } else if let Some(ser) = state.serprog.as_mut() {
                sfdp::discover_sfdp(|addr, len| sfdp_read_serprog(ser, addr, len))?
                    .map(|params| synthesize_sfdp_chip(&jedec_id, &params))
            } else {
                None
            };

            if let Some(info) = sfdp_match {
                let text = format!(
                    "✅ SFDP 兜底匹配成功！\n厂商: {}\n型号: {}\n容量: {}\n页大小: {} 字节\n协议: {}\nJEDEC: {}（未收录，参数来自 SFDP）\n（设备: {}）",
                    info.vendor,
                    info.model,
                    format_human_size(info.size),
                    info.page,
                    info.protocol,
                    jedec_id,
                    state.connected_device.as_deref().unwrap_or("未知")
                );
                let detected = info.clone();
                let result = ChipDetectResult {
                    text,
                    info: Some(chip_info_to_detect(&info)),
                };
                state.detected = Some(detected);
                return Ok(result);
            }

            let raw = probes
                .iter()
                .map(|p| {
                    format!(
                        "{:02X}{:02X}{:02X}{:02X}{:02X}",
                        p[0], p[1], p[2], p[3], p[4]
                    )
                })
                .collect::<Vec<_>>()
                .join(" / ");
            state.detected = None;
            Ok(ChipDetectResult {
                text: format!("❌ 未在芯片库中找到 ID（原始: {}）", raw),
                info: None,
            })
        }
    }
}
