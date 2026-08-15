#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ch34x;
mod chiplib;
mod dialogs;
mod protocols;
mod serprog;

use ch34x::{Ch34xDevice, Ch34xSettings, ChipKind, DeviceMode};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, State};

#[derive(Serialize)]
struct ChipDetectResult {
    text: String,
    info: Option<ChipDetectInfo>,
}

#[derive(Serialize)]
struct ChipDetectInfo {
    id: String,
    vendor: String,
    model: String,
    protocol: String,
    size: u64,
    page: u32,
    sector: Option<u64>,
    block: Option<u64>,
    addr4bit: Option<u32>,
    vcc: Option<String>,
    spare: Option<u64>,
    #[serde(rename = "pagesPerBlock")]
    pages_per_block: Option<u32>,
    #[serde(rename = "isBmm")]
    is_bmm: Option<bool>,
}

#[derive(Clone, Serialize)]
struct ReadProgressEvent {
    done: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
struct WriteProgressEvent {
    done: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
struct VerifyProgressEvent {
    done: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
struct BadBlockProgressEvent {
    done: u32,
    total: u32,
}

#[derive(Serialize)]
struct BadBlockScanResult {
    total_blocks: u32,
    bad_blocks: Vec<u32>,
    bad_count: u32,
}

struct AppState {
    ch34x: Option<Ch34xSettings>,
    serprog: Option<serprog::Serprog>,
    lib: Option<chiplib::Chiplib>,
    connected_device: Option<String>,
    detected: Option<chiplib::ChipInfo>,
}

fn exe_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        std::env::current_dir().expect("无法获取工作目录")
    }
    #[cfg(not(debug_assertions))]
    {
        let exe = std::env::current_exe().expect("无法获取 exe 路径");
        exe.parent().unwrap().to_path_buf()
    }
}

fn get_lib(state: &AppState) -> Result<&chiplib::Chiplib, String> {
    state.lib.as_ref().ok_or("芯片库未加载".into())
}

// ═══════════════════════════════ CH34X SPI helpers (IMPROG style) ═════════════

/// NOR parameters used by the read/write/erase paths. Missing fields fall back
/// to safe defaults until the chip database is enriched with IMSProg fields.
struct NorParams {
    page: usize,
    _sector: usize,
    _block: usize,
    addr4b: bool,
    alg: u8,
}

fn nor_params(info: &chiplib::ChipInfo) -> NorParams {
    let page = info.page.max(1) as usize;
    // attr names follow IMSProg database semantics after the importer runs:
    // addr4bit: low nibble 1 = use 4-byte addressing, high nibble = algorithm
    // (0 default B7/E9, 1 Winbond, 2 Spansion BRWR).
    let addr4bit = info.attr_u32("addr4bit").unwrap_or(0) as u8;
    let addr4b = (addr4bit & 0x0F) != 0 || info.size > 0x00FF_FFFF;
    let alg = (addr4bit >> 4) & 0x0F;
    NorParams {
        page,
        _sector: info.attr_u32("sector").unwrap_or(4096) as usize,
        _block: info.attr_u32("block").unwrap_or(64 * 1024) as usize,
        addr4b,
        alg,
    }
}

fn spi_read_status(dev: &Ch34xDevice) -> Result<u8, String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x05])?;
    let mut status = [0xFFu8; 1];
    dev.spi_rx(&mut status)?;
    dev.cs_high()?;
    Ok(status[0])
}

/// IMSProg snor_wait_ready(): poll RDSR until WIP|EPE|WEL are all clear.
fn spi_wait_ready(dev: &Ch34xDevice, timeout_ms: u64) -> Result<(), String> {
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

fn spi_write_enable(dev: &Ch34xDevice) -> Result<(), String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x06])?;
    dev.cs_high()
}

fn spi_write_disable(dev: &Ch34xDevice) -> Result<(), String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x04])?;
    dev.cs_high()
}

/// IMSProg snor_unprotect(): clear BP0-BP2 when they are set.
fn spi_unprotect(dev: &Ch34xDevice) -> Result<(), String> {
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
fn spi_4byte_mode(dev: &Ch34xDevice, alg: u8, enable: bool) -> Result<(), String> {
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

/// IMSProg snor_read_devid(): JEDEC ID, 5 bytes like IMSProg.
fn spi_read_jedec(dev: &Ch34xDevice) -> Result<[u8; 5], String> {
    dev.cs_low()?;
    dev.spi_tx(&[0x9F])?;
    let mut id = [0xFFu8; 5];
    dev.spi_rx(&mut id)?;
    dev.cs_high()?;
    Ok(id)
}

/// Open the selected programmer. The returned handle owns the USB device and
/// closes it on drop (per-operation lifecycle, same as IMSProg).
fn open_ch34x(state: &AppState) -> Result<Ch34xDevice, String> {
    open_ch34x_mode(state, DeviceMode::Spi)
}

fn open_ch34x_mode(state: &AppState, mode: DeviceMode) -> Result<Ch34xDevice, String> {
    let settings = state
        .ch34x
        .as_ref()
        .ok_or("没有可用的 CH34X 编程器，请先连接")?;
    Ch34xDevice::open_with_mode(settings, mode)
}

fn parse_nand_bad_block_mode(value: Option<&str>) -> protocols::NandBadBlockMode {
    match value {
        Some("skip") => protocols::NandBadBlockMode::Skip,
        Some("bypass") => protocols::NandBadBlockMode::Bypass,
        Some("ignore") => protocols::NandBadBlockMode::Ignore,
        _ => protocols::NandBadBlockMode::Ignore,
    }
}

fn scan_nand_bad_blocks_for_mode(
    dev: &Ch34xDevice,
    info: &chiplib::ChipInfo,
    app: &tauri::AppHandle,
    mode: protocols::NandBadBlockMode,
) -> Result<Vec<u32>, String> {
    if mode == protocols::NandBadBlockMode::Ignore {
        return Ok(Vec::new());
    }
    let params = protocols::ChipParams::from_info(info);
    protocols::nand_scan_bad_blocks(dev, &params, info.size, &mut |done, total| {
        app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
            .ok();
    })
}

// ═══════════════════════════════ serprog helpers ═════════════════════════════

fn serprog_wait_ready(ser: &mut serprog::Serprog, timeout_ms: u64) -> Result<(), String> {
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

// ═══════════════════════════════ Tauri commands ══════════════════════════════

#[tauri::command]
fn load_chip_lib(state: State<'_, Mutex<AppState>>) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    if s.lib.is_some() {
        return Ok("芯片库已加载".to_string());
    }
    let base = exe_dir();
    let xml_path = base.join("chiplib.xml");
    let bin_path = base.join("chiplib.bin");
    let lib = chiplib::Chiplib::load_auto(xml_path.to_str().unwrap(), bin_path.to_str().unwrap())?;
    s.lib = Some(lib);
    Ok("芯片库加载成功".to_string())
}

#[tauri::command]
fn initialize(
    state: State<'_, Mutex<AppState>>,
    kind: String,
    vcc_18v: bool,
    spi_mode: u8,
    freq_khz: u32,
) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;

    let chip_kind = match kind.as_str() {
        "ch341" => ChipKind::Ch341A,
        "ch347" => ChipKind::Ch347T,
        "ch347f" => ChipKind::Ch347F,
        _ => return Err("未知编程器类型".into()),
    };
    if spi_mode > 3 {
        return Err("SPI 模式必须在 0-3 之间".into());
    }
    if !(469..=60_000).contains(&freq_khz) {
        return Err("SPI 频率超出范围".into());
    }

    let settings = Ch34xSettings {
        kind: chip_kind,
        spi_mode,
        freq_khz,
        vcc_18v,
    };

    // Per-operation lifecycle: verify that the device opens now, then close.
    {
        let dev = Ch34xDevice::open(&settings)?;
        // Confirms the SPI stream works end to end (chip may be absent).
        let _ = spi_read_jedec(&dev)?;
    }

    let vcc_label = if settings.vcc_18v { " (1.8V)" } else { "" };
    s.ch34x = Some(settings);
    s.serprog = None;
    let base_name = match chip_kind {
        ChipKind::Ch341A => "CH341A Programmer",
        ChipKind::Ch347T => "CH347T Programmer",
        ChipKind::Ch347F => "CH347F Programmer",
    };
    let name = format!("{}{}", base_name, vcc_label);
    s.connected_device = Some(name.clone());
    Ok(format!("已连接: {}", name))
}

#[tauri::command]
fn connect_serprog(state: State<'_, Mutex<AppState>>, port: String) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let dev = serprog::Serprog::open(&port)?;
    let info = format!("serprog ({})", port);
    // 切换后端时清掉旧 CH34X，避免后续操作仍优先走 CH34X
    s.ch34x = None;
    s.detected = None;
    s.connected_device = Some(info.clone());
    s.serprog = Some(dev);
    Ok(format!("已连接: {}", info))
}

#[tauri::command]
fn detect_chip(state: State<'_, Mutex<AppState>>) -> Result<ChipDetectResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut id = [0xFFu8; 5];

    if s.ch34x.is_some() {
        let dev = open_ch34x(&s)?;
        id = spi_read_jedec(&dev)?;
    } else if let Some(ser) = &mut s.serprog {
        let id_bytes = ser.spi_command(&[0x9F], 3)?;
        id[0] = id_bytes[0];
        id[1] = id_bytes[1];
        id[2] = id_bytes[2];
    } else {
        return Err("没有可用的编程器，请先初始化或连接 serprog".into());
    }

    let id_hex = format!("{:02X}{:02X}{:02X}", id[0], id[1], id[2]);
    let lib = get_lib(&s)?;
    match lib.find_by_id(&id_hex) {
        Some(info) => {
            let text = format!(
                "✅ 芯片匹配成功！\n厂商: {}\n型号: {}\n容量: {} MB\n页大小: {} 字节\n协议: {}\n（设备: {}）",
                info.vendor,
                info.model,
                info.size / 1024 / 1024,
                info.page,
                info.protocol,
                s.connected_device.as_deref().unwrap_or("未知")
            );
            let detected = info.clone();
            let result = ChipDetectResult {
                text,
                info: Some(chip_info_to_detect(&info)),
            };
            s.detected = Some(detected);
            Ok(result)
        }
        None => {
            s.detected = None;
            Ok(ChipDetectResult {
                text: format!("❌ 未在芯片库中找到 ID: {}", id_hex),
                info: None,
            })
        }
    }
}

#[tauri::command]
fn scan_bad_blocks(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<BadBlockScanResult, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let info = s.detected.clone().ok_or("请先检测或选择 SPI NAND 芯片")?;
    if info.protocol != "SPI_NAND" {
        return Err("当前芯片不是 SPI NAND".into());
    }
    if s.ch34x.is_none() {
        return Err("坏块扫描目前仅支持 CH34X 后端，serprog 后端待实现".into());
    }

    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    let params = protocols::ChipParams::from_info(&info);
    let block_size = (params.block as u64).max(params.page as u64);
    let total_blocks = info.size.div_ceil(block_size).min(u32::MAX as u64) as u32;
    let bad_blocks =
        protocols::nand_scan_bad_blocks(&dev, &params, info.size, &mut |done, total| {
            app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
                .ok();
        })?;
    let bad_count = bad_blocks.len() as u32;
    Ok(BadBlockScanResult {
        total_blocks,
        bad_blocks,
        bad_count,
    })
}

#[tauri::command]
fn chip_erase(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    bad_block_mode: Option<String>,
) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;

    if s.ch34x.is_some() {
        let detected = s.detected.clone();
        let info = match detected.as_ref() {
            Some(info) => info.clone(),
            None => {
                // No detection cached: default to NOR behaviour.
                let dev = open_ch34x(&s)?;
                spi_wait_ready(&dev, 2000)?;
                spi_write_enable(&dev)?;
                spi_unprotect(&dev)?;
                spi_write_enable(&dev)?;
                dev.cs_low()?;
                dev.spi_tx(&[0xC7])?;
                dev.cs_high()?;
                spi_wait_ready(&dev, 120_000)?;
                spi_write_disable(&dev)?;
                return Ok("全片擦除完成".to_string());
            }
        };
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {
                let dev = open_ch34x_mode(&s, bus)?;
                spi_wait_ready(&dev, 2000)?;
                spi_write_enable(&dev)?;
                spi_unprotect(&dev)?;
                spi_write_enable(&dev)?;
                dev.cs_low()?;
                dev.spi_tx(&[0xC7])?;
                dev.cs_high()?;
                spi_wait_ready(&dev, 120_000)?;
                spi_write_disable(&dev)?;
            }
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(&s, bus)?;
                protocols::s95_erase(&dev)?;
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(&s, bus)?;
                protocols::at45_erase(&dev)?;
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                let bad_blocks = if s.ch34x.is_some() {
                    scan_nand_bad_blocks_for_mode(&dev, &info, &app, mode)?
                } else {
                    Vec::new()
                };
                protocols::nand_erase(&dev, &params, info.size, &bad_blocks)?;
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                protocols::i2c_erase(&dev, &params, info.size)?;
            }
            "Microwire" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                protocols::mw_erase(&dev, &params)?;
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    } else if let Some(ser) = &mut s.serprog {
        ser.spi_command(&[0x06], 0)?;
        ser.spi_command(&[0xC7], 0)?;
        serprog_wait_ready(ser, 120_000)?;
    } else {
        return Err("没有可用的编程器，请先初始化".into());
    }

    Ok("全片擦除完成".to_string())
}

/// NOR read, IMSProg style: manual CS, 0x03 read, optional 4-byte address mode
/// (B7/E9 or Spansion BRWR), progress events.
#[tauri::command]
fn read_chip(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<String>,
) -> Result<Vec<u8>, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;

    if s.serprog.is_some() && s.ch34x.is_none() {
        // serprog path: two-phase write-then-read, unchanged.
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = size > 0x00FF_FFFF;
        let cmd_read: u8 = if use_4b { 0x13 } else { 0x03 };
        let make_header = |addr: u64| -> Vec<u8> {
            let mut h = vec![cmd_read];
            if use_4b {
                h.push(((addr >> 24) & 0xFF) as u8);
            }
            h.push(((addr >> 16) & 0xFF) as u8);
            h.push(((addr >> 8) & 0xFF) as u8);
            h.push((addr & 0xFF) as u8);
            h
        };
        let total = size.saturating_sub(start_addr);
        let mut out = Vec::with_capacity(total as usize);
        let mut offset: u64 = 0;
        let read_chunk_max = ser.max_read_len().min(4096);
        while offset < total {
            let addr = start_addr + offset;
            let hdr = make_header(addr);
            let chunk = (total - offset).min(read_chunk_max as u64) as usize;
            let data = ser
                .spi_command(&hdr, chunk)
                .map_err(|e| format!("读取失败 @ 0x{:08X}: {}", addr, e))?;
            if data.len() != chunk {
                return Err(format!(
                    "serprog 读取长度不符 @ 0x{:08X}: 预期 {} 实际 {}",
                    addr,
                    chunk,
                    data.len()
                ));
            }
            out.extend_from_slice(&data);
            offset += chunk as u64;
            app.emit(
                "read_progress",
                ReadProgressEvent {
                    done: offset,
                    total,
                },
            )
            .ok();
        }
        return Ok(out);
    }

    // Non-NOR protocols dispatch to the ported IMSProg command sequences.
    if let Some(info) = s.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {}
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::s95_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| {
                        app.emit("read_progress", ReadProgressEvent { done, total })
                            .ok();
                    },
                );
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::at45_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| {
                        app.emit("read_progress", ReadProgressEvent { done, total })
                            .ok();
                    },
                );
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                let bad_blocks = scan_nand_bad_blocks_for_mode(&dev, info, &app, mode)?;
                return protocols::nand_read(
                    &dev,
                    &params,
                    size,
                    &bad_blocks,
                    &mut |done, total| {
                        app.emit("read_progress", ReadProgressEvent { done, total })
                            .ok();
                    },
                );
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::i2c_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| {
                        app.emit("read_progress", ReadProgressEvent { done, total })
                            .ok();
                    },
                );
            }
            "Microwire" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::mw_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| {
                        app.emit("read_progress", ReadProgressEvent { done, total })
                            .ok();
                    },
                );
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    }

    // CH34X path: IMPROG manual CS sequence.
    let params = match s.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: size > 0x00FF_FFFF,
            alg: 0,
        },
    };

    let dev = open_ch34x(&s)?;
    spi_wait_ready(&dev, 1000)?;
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let total = size.saturating_sub(start_addr);
    let mut out = Vec::with_capacity(total as usize);
    let mut offset: u64 = 0;
    let chunk_limit = dev.spi_frame_limit();

    while offset < total {
        let addr = start_addr + offset;
        let hdr_len = if params.addr4b { 5 } else { 4 };
        let chunk = (total - offset).min((chunk_limit.saturating_sub(hdr_len)) as u64) as usize;

        dev.cs_low()?;
        let mut hdr = vec![0x03u8];
        if params.addr4b {
            hdr.push(((addr >> 24) & 0xFF) as u8);
        }
        hdr.push(((addr >> 16) & 0xFF) as u8);
        hdr.push(((addr >> 8) & 0xFF) as u8);
        hdr.push((addr & 0xFF) as u8);
        dev.spi_tx(&hdr)
            .map_err(|e| format!("读取失败 @ 0x{:08X}: {}", addr, e))?;

        let start = out.len();
        out.resize(start + chunk, 0xFF);
        dev.spi_rx(&mut out[start..start + chunk])
            .map_err(|e| format!("读取失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;

        offset += chunk as u64;
        app.emit(
            "read_progress",
            ReadProgressEvent {
                done: offset,
                total,
            },
        )
        .ok();
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }

    Ok(out)
}

#[tauri::command]
fn write_chip(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    data: Vec<u8>,
    start_addr: u64,
    force_segmented: Option<bool>,
    bad_block_mode: Option<String>,
) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let total = data.len();

    if s.serprog.is_some() && s.ch34x.is_none() {
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = (start_addr + total as u64) > 0x00FF_FFFF;
        let make_header = |addr: u64| -> Vec<u8> {
            let mut h = vec![0x02u8];
            if use_4b {
                h.push(((addr >> 24) & 0xFF) as u8);
            }
            h.push(((addr >> 16) & 0xFF) as u8);
            h.push(((addr >> 8) & 0xFF) as u8);
            h.push((addr & 0xFF) as u8);
            h
        };
        let mut offset: usize = 0;
        while offset < total {
            let addr = start_addr + offset as u64;
            let hdr = make_header(addr);
            let chunk = 256
                .min(total - offset)
                .min(ser.max_write_len().saturating_sub(hdr.len()).max(1));
            ser.spi_command(&[0x06], 0)?;
            let mut frame = Vec::with_capacity(hdr.len() + chunk);
            frame.extend_from_slice(&hdr);
            frame.extend_from_slice(&data[offset..offset + chunk]);
            ser.spi_command(&frame, 0)?;
            serprog_wait_ready(ser, 100)?;
            offset += chunk;
            app.emit(
                "write_progress",
                WriteProgressEvent {
                    done: offset as u64,
                    total: total as u64,
                },
            )
            .ok();
        }
        return Ok(format!("写入完成，共 {} 字节", total));
    }

    // Non-NOR protocols dispatch to the ported IMSProg command sequences.
    if let Some(info) = s.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {}
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::s95_write(
                    &dev,
                    &params,
                    &data,
                    start_addr,
                    &mut |done, total| {
                        app.emit("write_progress", WriteProgressEvent { done, total })
                            .ok();
                    },
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::at45_write(
                    &dev,
                    &params,
                    &data,
                    start_addr,
                    &mut |done, total| {
                        app.emit("write_progress", WriteProgressEvent { done, total })
                            .ok();
                    },
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                let bad_blocks = scan_nand_bad_blocks_for_mode(&dev, info, &app, mode)?;
                return protocols::nand_write(
                    &dev,
                    &params,
                    &data,
                    info.size,
                    &bad_blocks,
                    mode,
                    force_segmented.unwrap_or(false),
                    &mut |done, total| {
                        app.emit("write_progress", WriteProgressEvent { done, total })
                            .ok();
                    },
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::i2c_write(
                    &dev,
                    &params,
                    &data,
                    start_addr,
                    &mut |done, total| {
                        app.emit("write_progress", WriteProgressEvent { done, total })
                            .ok();
                    },
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "Microwire" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::mw_write(
                    &dev,
                    &params,
                    &data,
                    start_addr,
                    &mut |done, total| {
                        app.emit("write_progress", WriteProgressEvent { done, total })
                            .ok();
                    },
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    }

    // CH34X path: IMPROG page program sequence.
    let dev = open_ch34x(&s)?;
    let params = match s.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: (start_addr + total as u64) > 0x00FF_FFFF,
            alg: 0,
        },
    };

    spi_wait_ready(&dev, 2000)?;
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let mut offset: usize = 0;
    while offset < total {
        let addr = start_addr + offset as u64;
        let chunk = params.page.min(total - offset);

        spi_wait_ready(&dev, 2000)?;
        spi_write_enable(&dev)?;

        dev.cs_low()?;
        let mut frame = Vec::with_capacity(5 + chunk);
        frame.push(0x02u8);
        if params.addr4b {
            frame.push(((addr >> 24) & 0xFF) as u8);
        }
        frame.push(((addr >> 16) & 0xFF) as u8);
        frame.push(((addr >> 8) & 0xFF) as u8);
        frame.push((addr & 0xFF) as u8);
        frame.extend_from_slice(&data[offset..offset + chunk]);
        dev.spi_tx(&frame)
            .map_err(|e| format!("写入失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;
        spi_wait_ready(&dev, 100)?;

        offset += chunk;
        app.emit(
            "write_progress",
            WriteProgressEvent {
                done: offset as u64,
                total: total as u64,
            },
        )
        .ok();
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }
    spi_write_disable(&dev)?;

    Ok(format!("写入完成，共 {} 字节", total))
}

#[tauri::command]
fn verify_chip(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    data: Vec<u8>,
    start_addr: u64,
    bad_block_mode: Option<String>,
) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let total = data.len() as u64;

    if s.serprog.is_some() && s.ch34x.is_none() {
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = (start_addr + total) > 0x00FF_FFFF;
        let cmd_read: u8 = if use_4b { 0x13 } else { 0x03 };
        let make_header = |addr: u64| -> Vec<u8> {
            let mut h = vec![cmd_read];
            if use_4b {
                h.push(((addr >> 24) & 0xFF) as u8);
            }
            h.push(((addr >> 16) & 0xFF) as u8);
            h.push(((addr >> 8) & 0xFF) as u8);
            h.push((addr & 0xFF) as u8);
            h
        };
        let mut offset: u64 = 0;
        let read_chunk_max = ser.max_read_len().min(4096);
        while offset < total {
            let addr = start_addr + offset;
            let hdr = make_header(addr);
            let chunk = (total - offset).min(read_chunk_max as u64) as usize;
            let buf = ser
                .spi_command(&hdr, chunk)
                .map_err(|e| format!("校验读取失败 @ 0x{:08X}: {}", addr, e))?;
            if buf.len() != chunk {
                return Err(format!(
                    "serprog 读取长度不符 @ 0x{:08X}: 预期 {} 实际 {}",
                    addr,
                    chunk,
                    buf.len()
                ));
            }
            for (i, actual) in buf.iter().enumerate() {
                let expected = data[offset as usize + i];
                if expected != *actual {
                    let fail_addr = format!("0x{:08X}", addr + i as u64);
                    return Err(format!(
                        "校验失败 @ {}: 期望 0x{:02X}, 读到 0x{:02X}",
                        fail_addr, expected, actual
                    ));
                }
            }
            offset += chunk as u64;
            app.emit(
                "verify_progress",
                VerifyProgressEvent {
                    done: offset,
                    total,
                },
            )
            .ok();
        }
        return Ok("校验通过".to_string());
    }

    // Non-NOR protocols: read back with the ported command sequences and
    // compare in-place.
    if let Some(info) = s.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        let params = protocols::ChipParams::from_info(info);
        let buf: Vec<u8> = match info.protocol.as_str() {
            "SPI_NOR" => Vec::new(),
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(&s, bus)?;
                protocols::s95_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(&s, bus)?;
                protocols::at45_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                let bad_blocks = scan_nand_bad_blocks_for_mode(&dev, info, &app, mode)?;
                protocols::nand_read(&dev, &params, total, &bad_blocks, &mut |_, _| {})?
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(&s, bus)?;
                protocols::i2c_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            "Microwire" => {
                let dev = open_ch34x_mode(&s, bus)?;
                protocols::mw_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        };

        if !buf.is_empty() {
            if buf.len() != data.len() {
                return Err(format!(
                    "校验读取长度不符: 预期 {} 实际 {}",
                    data.len(),
                    buf.len()
                ));
            }
            let mut offset = 0usize;
            while offset < buf.len() {
                let end = (offset + 4096).min(buf.len());
                for i in offset..end {
                    if buf[i] != data[i] {
                        let fail_addr = format!("0x{:08X}", start_addr + i as u64);
                        return Err(format!(
                            "校验失败 @ {}: 期望 0x{:02X}, 读到 0x{:02X}",
                            fail_addr, data[i], buf[i]
                        ));
                    }
                }
                offset = end;
                app.emit(
                    "verify_progress",
                    VerifyProgressEvent {
                        done: offset as u64,
                        total,
                    },
                )
                .ok();
            }
            return Ok("校验通过".to_string());
        }
    }

    // CH34X path: read back and compare.
    let dev = open_ch34x(&s)?;
    let params = match s.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: (start_addr + total) > 0x00FF_FFFF,
            alg: 0,
        },
    };

    spi_wait_ready(&dev, 1000)?;
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let mut offset: u64 = 0;
    let chunk_limit = dev.spi_frame_limit();
    while offset < total {
        let addr = start_addr + offset;
        let hdr_len = if params.addr4b { 5 } else { 4 };
        let chunk = (total - offset).min((chunk_limit.saturating_sub(hdr_len)) as u64) as usize;

        dev.cs_low()?;
        let mut hdr = vec![0x03u8];
        if params.addr4b {
            hdr.push(((addr >> 24) & 0xFF) as u8);
        }
        hdr.push(((addr >> 16) & 0xFF) as u8);
        hdr.push(((addr >> 8) & 0xFF) as u8);
        hdr.push((addr & 0xFF) as u8);
        dev.spi_tx(&hdr)
            .map_err(|e| format!("校验读取失败 @ 0x{:08X}: {}", addr, e))?;

        let mut buf = vec![0xFFu8; chunk];
        dev.spi_rx(&mut buf)
            .map_err(|e| format!("校验读取失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;

        for (i, actual) in buf.iter().enumerate() {
            let expected = data[offset as usize + i];
            if expected != *actual {
                let fail_addr = format!("0x{:08X}", addr + i as u64);
                return Err(format!(
                    "校验失败 @ {}: 期望 0x{:02X}, 读到 0x{:02X}",
                    fail_addr, expected, actual
                ));
            }
        }

        offset += chunk as u64;
        app.emit(
            "verify_progress",
            VerifyProgressEvent {
                done: offset,
                total,
            },
        )
        .ok();
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }

    Ok("校验通过".to_string())
}

fn chip_info_to_detect(info: &chiplib::ChipInfo) -> ChipDetectInfo {
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
    }
}

#[tauri::command]
fn get_chip_info(
    state: State<'_, Mutex<AppState>>,
    protocol: String,
    vendor: String,
    model: String,
) -> Result<ChipDetectInfo, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let lib = get_lib(&s)?;
    let info = lib
        .find_by_model(&protocol, &vendor, &model)
        .ok_or_else(|| format!("芯片库中未找到: {} / {}", vendor, model))?;
    Ok(chip_info_to_detect(&info))
}

#[tauri::command]
fn get_chip_types(state: State<'_, Mutex<AppState>>) -> Result<Vec<String>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let lib = get_lib(&s)?;
    Ok(lib.list_protocols())
}

#[tauri::command]
fn get_chip_vendors(
    state: State<'_, Mutex<AppState>>,
    protocol: String,
) -> Result<Vec<String>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let lib = get_lib(&s)?;
    Ok(lib.list_vendors(&protocol))
}

#[tauri::command]
fn get_chip_models(
    state: State<'_, Mutex<AppState>>,
    protocol: String,
    vendor: String,
) -> Result<Vec<String>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let lib = get_lib(&s)?;
    Ok(lib.list_models(&protocol, &vendor))
}

#[derive(Serialize)]
struct ChipDbImportReport {
    dat_records: usize,
    matched_by_id: usize,
    matched_by_name: usize,
    entries_updated: usize,
    saved_to: String,
}

#[tauri::command]
fn convert_chip_lib(state: State<'_, Mutex<AppState>>) -> Result<String, String> {
    let _s = state.lock().map_err(|e| e.to_string())?;
    let base = exe_dir();
    let xml_path = base.join("chiplib.xml");
    let bin_path = base.join("chiplib.bin");

    chiplib::Chiplib::convert_xml_to_bin(xml_path.to_str().unwrap(), bin_path.to_str().unwrap())?;
    Ok(format!("芯片库已成功转换为 {}", bin_path.display()))
}

/// Import IMSProg.Dat fields into chiplib.bin (fill missing values only).
#[tauri::command]
fn import_chip_db(
    state: State<'_, Mutex<AppState>>,
    dat_path: String,
) -> Result<ChipDbImportReport, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let base = exe_dir();
    let bin_path = base.join("chiplib.bin");

    let mut lib = chiplib::Chiplib::load_bin(bin_path.to_str().unwrap())?;
    let stats = lib.import_imsprog_dat(&dat_path)?;
    lib.save_bin(bin_path.to_str().unwrap())?;
    s.lib = Some(lib);

    Ok(ChipDbImportReport {
        dat_records: stats.dat_records,
        matched_by_id: stats.matched_by_id,
        matched_by_name: stats.matched_by_name,
        entries_updated: stats.entries_updated,
        saved_to: bin_path.display().to_string(),
    })
}

#[tauri::command]
fn open_file_dialog() -> Result<Option<String>, String> {
    dialogs::open_file()
}

#[tauri::command]
fn save_file_dialog(default_name: String, default_ext: String) -> Result<Option<String>, String> {
    dialogs::save_file(&default_name, &default_ext)
}

#[tauri::command]
fn read_file(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|e| format!("读取文件失败 {}: {}", path, e))
}

#[tauri::command]
fn write_file(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, &data).map_err(|e| format!("写入文件失败 {}: {}", path, e))
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState {
            ch34x: None,
            serprog: None,
            lib: None,
            connected_device: None,
            detected: None,
        }))
        .invoke_handler(tauri::generate_handler![
            load_chip_lib,
            initialize,
            connect_serprog,
            detect_chip,
            scan_bad_blocks,
            chip_erase,
            read_chip,
            write_chip,
            verify_chip,
            get_chip_types,
            get_chip_vendors,
            get_chip_models,
            get_chip_info,
            convert_chip_lib,
            import_chip_db,
            open_file_dialog,
            save_file_dialog,
            read_file,
            write_file,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
