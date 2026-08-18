// Shared application state and device-level operations that do not depend on
// the UI transport. The Tauri command layer stays a thin adapter on top of
// these functions so the state can be driven from any frontend later.

use serde::Serialize;
use std::collections::HashMap;

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
