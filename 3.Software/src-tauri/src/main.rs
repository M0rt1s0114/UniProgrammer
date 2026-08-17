#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autodetect;
mod ch34x;
mod chiplib;
mod dialogs;
mod firmware;
mod protocols;
mod serprog;
mod settings;

use ch34x::{Ch34xDevice, Ch34xSettings, ChipKind, DeviceMode};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, State, WindowEvent};

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
    #[serde(rename = "dummyMode")]
    dummy_mode: Option<String>,
    #[serde(rename = "readMode")]
    read_mode: Option<String>,
    #[serde(rename = "writeMode")]
    write_mode: Option<String>,
    feature: Option<u32>,
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

#[derive(Clone, Serialize)]
struct BlankCheckProgressEvent {
    done: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlankCheckResult {
    blank: bool,
    checked: u64,
    first_non_blank: Option<u64>,
}

#[derive(Clone, Serialize)]
struct EraseProgressEvent {
    done: u64,
    total: u64,
    phase: String,
    message: String,
    #[serde(rename = "elapsedMs")]
    elapsed_ms: Option<u64>,
}

#[derive(Serialize)]
struct BadBlockScanResult {
    total_blocks: u32,
    bad_blocks: Vec<u32>,
    bad_count: u32,
}

#[derive(Serialize)]
struct RawBytesResult {
    length: usize,
    hex: String,
    bytes: Vec<u8>,
}

#[derive(Serialize)]
struct FirmwareLoadResult {
    length: usize,
    bytes: Vec<u8>,
    format: String,
}

#[derive(Serialize)]
struct BbmLutResult {
    length: usize,
    hex: String,
    entries: Vec<protocols::BbmLutEntry>,
}

fn raw_bytes_result(bytes: Vec<u8>) -> RawBytesResult {
    let hex: String = bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    RawBytesResult {
        length: bytes.len(),
        hex,
        bytes,
    }
}

struct AppState {
    ch34x: Option<Ch34xSettings>,
    serprog: Option<serprog::Serprog>,
    lib: Option<chiplib::Chiplib>,
    connected_device: Option<String>,
    detected: Option<chiplib::ChipInfo>,
    /// Last serial-port snapshot. Serial probing only runs again when this
    /// list changes, so the hotplug poll never chats with every COM port.
    last_serial_ports: Vec<String>,
    /// Last serprog probe result, reused while the port list is unchanged.
    cached_serprog: Vec<autodetect::ProgrammerCandidate>,
    /// Mirrored from the frontend: true while read/write/erase/verify/auto
    /// flow is executing. Used by the Rust close-requested handler.
    operation_running: bool,
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

fn format_human_size(size: u64) -> String {
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

fn get_lib(state: &AppState) -> Result<&chiplib::Chiplib, String> {
    state.lib.as_ref().ok_or("芯片库未加载".into())
}

// ═══════════════════════════════ CH34X SPI helpers (IMPROG style) ═════════════

/// 3-byte addressing covers exactly 16 MiB (addresses 0x000000..0xFFFFFF).
/// 4-byte mode is only required above 16 MiB; 16 MiB chips such as
/// EN25QH128 must stay in 3-byte mode.
fn nor_requires_4byte(size: u64) -> bool {
    size > 0x0100_0000
}

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

fn emit_erase_progress_event(
    app: &tauri::AppHandle,
    done: u64,
    total: u64,
    phase: &str,
    message: &str,
    elapsed_ms: Option<u64>,
) {
    app.emit(
        "erase_progress",
        EraseProgressEvent {
            done,
            total,
            phase: phase.to_string(),
            message: message.to_string(),
            elapsed_ms,
        },
    )
    .ok();
}

fn emit_erase_progress(app: &tauri::AppHandle, done: u64, total: u64, phase: &str, message: &str) {
    emit_erase_progress_event(app, done, total, phase, message, None);
}

/// Indeterminate progress: no real percentage, only a message and an elapsed
/// timer. Used while the chip is busy with a full-chip erase.
fn emit_erase_progress_elapsed(
    app: &tauri::AppHandle,
    phase: &str,
    message: &str,
    elapsed_ms: u64,
) {
    emit_erase_progress_event(app, 0, 0, phase, message, Some(elapsed_ms));
}

/// Same as `spi_wait_ready`, but reports elapsed time to the frontend every
/// 250 ms so a long full-chip erase never looks frozen.
fn spi_wait_ready_with_progress(
    dev: &Ch34xDevice,
    timeout_ms: u64,
    app: &tauri::AppHandle,
    phase: &str,
    message_prefix: &str,
) -> Result<(), String> {
    let start = Instant::now();
    let mut last_report = start;
    loop {
        let status = spi_read_status(dev)?;
        if (status & 0x01 | status & 0x20 | status & 0x02) == 0 {
            return Ok(());
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms > timeout_ms {
            return Err("等待闪存就绪超时".into());
        }
        if last_report.elapsed() >= Duration::from_millis(250) {
            emit_erase_progress_elapsed(
                app,
                phase,
                &format!(
                    "{} · 最长等待 {:.0}s",
                    message_prefix,
                    timeout_ms as f64 / 1000.0
                ),
                elapsed_ms,
            );
            last_report = Instant::now();
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

/// SPI NAND read-ID variant: 9Fh + manufacturer-ID address byte 00h.
/// Some NAND vendors only output the ID after receiving the address byte;
/// the plain 9Fh probe then returns a leading FF byte.
fn spi_read_jedec_with_addr(dev: &Ch34xDevice) -> Result<[u8; 5], String> {
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
fn jedec_id_candidates(raw: &[u8]) -> Vec<String> {
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

fn prepare_bypass_if_needed(
    dev: &Ch34xDevice,
    info: &chiplib::ChipInfo,
    mode: protocols::NandBadBlockMode,
    bad_blocks: &[u32],
) -> Result<Vec<(u16, u16)>, String> {
    if mode != protocols::NandBadBlockMode::Bypass || bad_blocks.is_empty() {
        return Ok(Vec::new());
    }
    let params = protocols::ChipParams::from_info(info);
    protocols::nand_prepare_bypass_lut(dev, &params, info.size, bad_blocks)
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

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn initialize(
    state: State<'_, Mutex<AppState>>,
    kind: String,
    io_level_mv: u32,
    spi_mode: u8,
    freq_khz: u32,
    device_index: Option<u32>,
    usb_bus: Option<u8>,
    usb_address: Option<u8>,
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
    if ![1200, 1800, 2500, 3300].contains(&io_level_mv) {
        return Err("目标电平电压无效（支持 1.2/1.8/2.5/3.3V）".into());
    }

    let settings = Ch34xSettings {
        kind: chip_kind,
        spi_mode,
        freq_khz,
        // VCC 供电与 SPI/IO 信号电平绑定到同一目标轨
        io_level_mv,
        device_index: device_index.unwrap_or(0),
        usb_bus,
        usb_address,
    };

    // Per-operation lifecycle: verify that the device opens now, then close.
    {
        let dev = Ch34xDevice::open(&settings)?;
        // Confirms the SPI stream works end to end (chip may be absent).
        let _ = spi_read_jedec(&dev)?;
    }

    s.ch34x = Some(settings);
    s.serprog = None;
    let base_name = match chip_kind {
        ChipKind::Ch341A => "CH341A Programmer",
        ChipKind::Ch347T => "CH347T Programmer",
        ChipKind::Ch347F => "CH347F Programmer",
    };
    let name = base_name.to_string();
    s.connected_device = Some(name.clone());
    // TODO: VCC/IO 电平切换尚未与硬件绑定，先不输出目标电压日志；
    // 后续实现电压控制后再恢复为：
    // Ok(format!("已连接: {}（目标 VCC/IO 电平 {:.1}V）", name, io_level_v))
    Ok(format!("已连接: {}", name))
}

#[tauri::command]
async fn connect_serprog(
    state: State<'_, Mutex<AppState>>,
    port: String,
) -> Result<String, String> {
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

/// Scan for supported programmers.
///
/// USB results come back immediately; serprog probing runs in the background
/// and is delivered through the `serprog_scan_result` event. Port probing is
/// cached and only re-runs when the serial port list changes or the caller
/// forces it (`include_serprog`).
#[tauri::command]
async fn scan_programmers(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    include_serprog: bool,
    quick_serprog: bool,
) -> Result<Vec<autodetect::ProgrammerCandidate>, String> {
    let (candidates, probe_ports) = {
        let mut s = state.lock().map_err(|e| e.to_string())?;
        let mut candidates = autodetect::scan_ch34x();
        let ports: Vec<String> = serialport::available_ports()
            .map(|list| list.into_iter().map(|p| p.port_name).collect())
            .unwrap_or_default();
        let ports_changed = include_serprog || ports != s.last_serial_ports;
        if ports_changed {
            s.last_serial_ports = ports.clone();
            // 保留上一轮结果，避免串口探测完成前列表闪烁。
            candidates.extend(s.cached_serprog.clone());
            (candidates, Some(ports))
        } else {
            candidates.extend(s.cached_serprog.clone());
            (candidates, None)
        }
    };

    if let Some(ports) = probe_ports {
        let probe_app = app.clone();
        std::thread::spawn(move || {
            let found = autodetect::scan_serprog(&ports, quick_serprog);
            let state = probe_app.state::<Mutex<AppState>>();
            if let Ok(mut s) = state.lock() {
                s.cached_serprog = found.clone();
            }
            probe_app.emit("serprog_scan_result", found).ok();
        });
    }

    Ok(candidates)
}

#[tauri::command]
async fn detect_chip(state: State<'_, Mutex<AppState>>) -> Result<ChipDetectResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut probes: Vec<[u8; 5]> = Vec::new();

    if s.ch34x.is_some() {
        let dev = open_ch34x(&s)?;
        probes.push(spi_read_jedec(&dev)?);
        probes.push(spi_read_jedec_with_addr(&dev)?);
    } else if let Some(ser) = &mut s.serprog {
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

    let lib = get_lib(&s)?;
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
            s.detected = None;
            Ok(ChipDetectResult {
                text: format!("❌ 未在芯片库中找到 ID（原始: {}）", raw),
                info: None,
            })
        }
    }
}

#[tauri::command]
async fn scan_bad_blocks(
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
fn read_nand_uid(state: State<'_, Mutex<AppState>>) -> Result<RawBytesResult, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    require_nand_ch34x(&s)?;
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    Ok(raw_bytes_result(protocols::nand_read_uid(&dev, 64)?))
}

#[tauri::command]
fn read_nand_param_page(state: State<'_, Mutex<AppState>>) -> Result<RawBytesResult, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    require_nand_ch34x(&s)?;
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    Ok(raw_bytes_result(protocols::nand_read_param_page(&dev)?))
}

#[tauri::command]
fn read_nand_bbm_lut(state: State<'_, Mutex<AppState>>) -> Result<BbmLutResult, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    require_nand_ch34x(&s)?;
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    let (entries, raw) = protocols::nand_read_bbm_lut(&dev)?;
    let result = raw_bytes_result(raw);
    Ok(BbmLutResult {
        length: result.length,
        hex: result.hex,
        entries,
    })
}

#[tauri::command]
fn read_nand_otp_page(
    state: State<'_, Mutex<AppState>>,
    page: u32,
) -> Result<RawBytesResult, String> {
    if page > 63 {
        return Err("OTP 页号超出范围（0-63）".into());
    }
    let s = state.lock().map_err(|e| e.to_string())?;
    require_nand_ch34x(&s)?;
    let info = s.detected.as_ref().unwrap();
    let params = protocols::ChipParams::from_info(info);
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    Ok(raw_bytes_result(protocols::nand_read_otp_page(
        &dev, &params, page,
    )?))
}

#[tauri::command]
fn get_nand_ecc(state: State<'_, Mutex<AppState>>) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    require_nand_ch34x(&s)?;
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    protocols::nand_get_ecc(&dev)
}

#[tauri::command]
fn set_nand_ecc(state: State<'_, Mutex<AppState>>, enable: bool) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    require_nand_ch34x(&s)?;
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    protocols::nand_set_ecc(&dev, enable)?;
    protocols::nand_get_ecc(&dev)
}

#[derive(Serialize)]
struct At45PageModeResult {
    raw: u8,
    binary_page: bool,
}

#[tauri::command]
fn read_at45_page_mode(state: State<'_, Mutex<AppState>>) -> Result<At45PageModeResult, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let info = s
        .detected
        .as_ref()
        .ok_or("请先检测或选择 45 系列 DataFlash 芯片")?;
    if info.protocol != "SPI_DATA_45" {
        return Err("当前芯片不是 45 系列 DataFlash".into());
    }
    if s.ch34x.is_none() {
        return Err("此功能目前仅支持 CH34X 后端".into());
    }
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    let raw = protocols::at45_read_page_mode(&dev)?;
    Ok(At45PageModeResult {
        raw,
        binary_page: (raw & 0x01) != 0,
    })
}

#[tauri::command]
fn set_at45_page_mode(
    state: State<'_, Mutex<AppState>>,
    binary: bool,
) -> Result<At45PageModeResult, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let info = s
        .detected
        .as_ref()
        .ok_or("请先检测或选择 45 系列 DataFlash 芯片")?;
    if info.protocol != "SPI_DATA_45" {
        return Err("当前芯片不是 45 系列 DataFlash".into());
    }
    if s.ch34x.is_none() {
        return Err("此功能目前仅支持 CH34X 后端".into());
    }
    let dev = open_ch34x_mode(&s, DeviceMode::Spi)?;
    protocols::at45_set_page_mode(&dev, binary)?;
    let raw = protocols::at45_read_page_mode(&dev)?;
    Ok(At45PageModeResult {
        raw,
        binary_page: (raw & 0x01) != 0,
    })
}

fn require_nand_ch34x(state: &AppState) -> Result<(), String> {
    let info = state
        .detected
        .as_ref()
        .ok_or("请先检测或选择 SPI NAND 芯片")?;
    if info.protocol != "SPI_NAND" {
        return Err("当前芯片不是 SPI NAND".into());
    }
    if state.ch34x.is_none() {
        return Err("此功能目前仅支持 CH34X 后端".into());
    }
    Ok(())
}

#[tauri::command]
async fn chip_erase(
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
                emit_erase_progress(&app, 0, 0, "prepare", "等待芯片就绪...");
                spi_wait_ready(&dev, 2000)?;
                emit_erase_progress(&app, 0, 0, "prepare", "写使能 (WREN)...");
                spi_write_enable(&dev)?;
                emit_erase_progress(&app, 0, 0, "prepare", "解除写保护...");
                spi_unprotect(&dev)?;
                emit_erase_progress(&app, 0, 0, "prepare", "再次写使能 (WREN)...");
                spi_write_enable(&dev)?;
                emit_erase_progress(
                    &app,
                    0,
                    0,
                    "erase",
                    "已发送全片擦除命令 (C7h)，芯片内部擦除中...",
                );
                dev.cs_low()?;
                dev.spi_tx(&[0xC7])?;
                dev.cs_high()?;
                spi_wait_ready_with_progress(&dev, 120_000, &app, "erase", "全片擦除中")?;
                spi_write_disable(&dev)?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
                return Ok("全片擦除完成".to_string());
            }
        };
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {
                let dev = open_ch34x_mode(&s, bus)?;
                emit_erase_progress(&app, 0, 0, "prepare", "等待芯片就绪...");
                spi_wait_ready(&dev, 2000)?;
                emit_erase_progress(&app, 0, 0, "prepare", "写使能 (WREN)...");
                spi_write_enable(&dev)?;
                emit_erase_progress(&app, 0, 0, "prepare", "解除写保护...");
                spi_unprotect(&dev)?;
                emit_erase_progress(&app, 0, 0, "prepare", "再次写使能 (WREN)...");
                spi_write_enable(&dev)?;
                emit_erase_progress(
                    &app,
                    0,
                    0,
                    "erase",
                    "已发送全片擦除命令 (C7h)，芯片内部擦除中...",
                );
                dev.cs_low()?;
                dev.spi_tx(&[0xC7])?;
                dev.cs_high()?;
                spi_wait_ready_with_progress(&dev, 120_000, &app, "erase", "全片擦除中")?;
                spi_write_disable(&dev)?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
            }
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(&s, bus)?;
                emit_erase_progress(&app, 0, 0, "erase", "EEPROM 全片擦除中...");
                protocols::s95_erase(&dev)?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(&s, bus)?;
                emit_erase_progress(&app, 0, 0, "erase", "DataFlash 全片擦除中...");
                protocols::at45_erase(&dev)?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                emit_erase_progress(&app, 0, 0, "bad_block", "正在扫描坏块...");
                let bad_blocks = scan_nand_bad_blocks_for_mode(&dev, &info, &app, mode)?;
                let links = prepare_bypass_if_needed(&dev, &info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                protocols::nand_erase(&dev, &params, info.size, &op_bad, &mut |done, total| {
                    emit_erase_progress(
                        &app,
                        done as u64,
                        total as u64,
                        "erase",
                        &format!("SPI NAND 块擦除 {}/{}", done, total),
                    );
                })?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
                if !links.is_empty() {
                    return Ok(format!(
                        "全片擦除完成（已写入 {} 条 BBM 坏块映射）",
                        links.len()
                    ));
                }
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                emit_erase_progress(&app, 0, 0, "erase", "I2C 芯片擦除中（写 0xFF）...");
                protocols::i2c_erase(&dev, &params, info.size, &mut |done, total| {
                    emit_erase_progress(
                        &app,
                        done,
                        total,
                        "erase",
                        &format!("I2C 擦除 {}/{} 字节", done, total),
                    );
                })?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
            }
            "Microwire" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                emit_erase_progress(&app, 0, 0, "erase", "Microwire 全片擦除中...");
                protocols::mw_erase(&dev, &params)?;
                emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    } else if let Some(ser) = &mut s.serprog {
        emit_erase_progress(&app, 0, 0, "prepare", "写使能 (WREN)...");
        ser.spi_command(&[0x06], 0)?;
        emit_erase_progress(
            &app,
            0,
            0,
            "erase",
            "已发送全片擦除命令 (C7h)，芯片内部擦除中...",
        );
        ser.spi_command(&[0xC7], 0)?;
        serprog_wait_ready(ser, 120_000)?;
        emit_erase_progress(&app, 1, 1, "done", "全片擦除完成");
    } else {
        return Err("没有可用的编程器，请先初始化".into());
    }

    Ok("全片擦除完成".to_string())
}

/// Return the absolute address of the first byte that is not 0xFF.
fn first_non_blank_byte(data: &[u8], base: u64) -> Option<u64> {
    data.iter()
        .position(|&b| b != 0xFF)
        .map(|index| base + index as u64)
}

/// Stream a blank check: read the requested range in chunks and stop at the
/// first non-0xFF byte. Never materialises the whole chip in memory.
#[tauri::command]
async fn blank_check(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<String>,
) -> Result<BlankCheckResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let total = size.saturating_sub(start_addr);
    if total == 0 {
        return Ok(BlankCheckResult {
            blank: true,
            checked: 0,
            first_non_blank: None,
        });
    }

    // serprog path: stream reads through O_SPIOP.
    if s.serprog.is_some() && s.ch34x.is_none() {
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = size > 0x0100_0000;
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
        let chunk_max = ser.max_read_len().min(4096);
        let mut offset: u64 = 0;
        while offset < total {
            let addr = start_addr + offset;
            let chunk = (total - offset).min(chunk_max as u64) as usize;
            let data = ser
                .spi_command(&make_header(addr), chunk)
                .map_err(|e| format!("查空读取失败 @ 0x{:08X}: {}", addr, e))?;
            if data.len() != chunk {
                return Err(format!(
                    "查空读取长度不符 @ 0x{:08X}: 预期 {} 实际 {}",
                    addr,
                    chunk,
                    data.len()
                ));
            }
            if let Some(pos) = first_non_blank_byte(&data, addr) {
                return Ok(BlankCheckResult {
                    blank: false,
                    checked: offset + pos - addr,
                    first_non_blank: Some(pos),
                });
            }
            offset += chunk as u64;
            app.emit(
                "blank_check_progress",
                BlankCheckProgressEvent {
                    done: offset,
                    total,
                },
            )
            .ok();
        }
        return Ok(BlankCheckResult {
            blank: true,
            checked: total,
            first_non_blank: None,
        });
    }

    // Small / non-streaming protocols reuse the existing read paths and scan
    // the returned buffer. Capped so a pathological request cannot OOM.
    if let Some(info) = s.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {}
            other => {
                if size > 256 * 1024 * 1024 {
                    return Err(format!("{} 协议查空暂不支持超过 256 MiB", other));
                }
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let data = match other {
                    "SPI_EEPROM" => protocols::s95_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| {
                            app.emit(
                                "blank_check_progress",
                                BlankCheckProgressEvent { done, total },
                            )
                            .ok();
                        },
                    )?,
                    "SPI_DATA_45" => protocols::at45_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| {
                            app.emit(
                                "blank_check_progress",
                                BlankCheckProgressEvent { done, total },
                            )
                            .ok();
                        },
                    )?,
                    "SPI_NAND" => {
                        let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                        let bad_blocks = scan_nand_bad_blocks_for_mode(&dev, info, &app, mode)?;
                        let links = prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                        let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                            Vec::new()
                        } else {
                            bad_blocks
                        };
                        let data = protocols::nand_read(
                            &dev,
                            &params,
                            size,
                            &op_bad,
                            &mut |done, total| {
                                app.emit(
                                    "blank_check_progress",
                                    BlankCheckProgressEvent { done, total },
                                )
                                .ok();
                            },
                        )?;
                        let _ = links;
                        data
                    }
                    "I2C" | "I2C_F-RAM" | "I2C_SPD" => protocols::i2c_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| {
                            app.emit(
                                "blank_check_progress",
                                BlankCheckProgressEvent { done, total },
                            )
                            .ok();
                        },
                    )?,
                    "Microwire" => protocols::mw_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| {
                            app.emit(
                                "blank_check_progress",
                                BlankCheckProgressEvent { done, total },
                            )
                            .ok();
                        },
                    )?,
                    other => return Err(format!("协议 {} 暂未实现查空", other)),
                };
                if let Some(pos) = first_non_blank_byte(&data, start_addr) {
                    return Ok(BlankCheckResult {
                        blank: false,
                        checked: pos - start_addr,
                        first_non_blank: Some(pos),
                    });
                }
                return Ok(BlankCheckResult {
                    blank: true,
                    checked: data.len() as u64,
                    first_non_blank: None,
                });
            }
        }
    }

    // CH34X SPI NOR: manual CS read, chunked, no whole-image buffer.
    let params = match s.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: size > 0x0100_0000,
            alg: 0,
        },
    };
    let dev = open_ch34x(&s)?;
    spi_wait_ready(&dev, 1000)?;
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let chunk_limit = dev.spi_frame_limit();
    let mut offset: u64 = 0;
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
            .map_err(|e| format!("查空读取失败 @ 0x{:08X}: {}", addr, e))?;
        let mut buf = vec![0xFFu8; chunk];
        dev.spi_rx(&mut buf)
            .map_err(|e| format!("查空读取失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;

        if let Some(pos) = first_non_blank_byte(&buf, addr) {
            if params.addr4b {
                spi_4byte_mode(&dev, params.alg, false)?;
            }
            return Ok(BlankCheckResult {
                blank: false,
                checked: offset + (pos - addr),
                first_non_blank: Some(pos),
            });
        }
        offset += chunk as u64;
        app.emit(
            "blank_check_progress",
            BlankCheckProgressEvent {
                done: offset,
                total,
            },
        )
        .ok();
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }
    Ok(BlankCheckResult {
        blank: true,
        checked: total,
        first_non_blank: None,
    })
}

/// NOR read, IMSProg style: manual CS, 0x03 read, optional 4-byte address mode
/// (B7/E9 or Spansion BRWR), progress events.
#[tauri::command]
async fn read_chip(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;

    if s.serprog.is_some() && s.ch34x.is_none() {
        // serprog path: two-phase write-then-read, unchanged.
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = size > 0x0100_0000;
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
        return Ok(tauri::ipc::Response::new(out));
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
                )
                .map(tauri::ipc::Response::new);
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
                )
                .map(tauri::ipc::Response::new);
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let mode = parse_nand_bad_block_mode(bad_block_mode.as_deref());
                let bad_blocks = scan_nand_bad_blocks_for_mode(&dev, info, &app, mode)?;
                prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                return protocols::nand_read(&dev, &params, size, &op_bad, &mut |done, total| {
                    app.emit("read_progress", ReadProgressEvent { done, total })
                        .ok();
                })
                .map(tauri::ipc::Response::new);
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
                )
                .map(tauri::ipc::Response::new);
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
                )
                .map(tauri::ipc::Response::new);
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
            addr4b: size > 0x0100_0000,
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

    Ok(tauri::ipc::Response::new(out))
}

/// Extract the raw bytes payload sent with `invoke('write_chip', uint8Array, ...)`.
/// Tauri delivers a top-level `Uint8Array`/`ArrayBuffer` body as `InvokeBody::Raw`,
/// which keeps multi-megabyte NAND images from being JSON-serialized as number arrays.
fn raw_request_bytes<'a>(request: &'a tauri::ipc::Request<'_>) -> Result<&'a [u8], String> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => Ok(bytes.as_slice()),
        tauri::ipc::InvokeBody::Json(_) => {
            Err("写入/校验数据必须以原始字节（Uint8Array/ArrayBuffer）发送".into())
        }
    }
}

/// Read a parameter that the frontend passed as an invoke option header.
fn request_header(request: &tauri::ipc::Request<'_>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[tauri::command]
async fn write_chip(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let data = raw_request_bytes(&request)?;
    let start_addr: u64 = request_header(&request, "x-start-addr")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let force_segmented: Option<bool> =
        request_header(&request, "x-force-segmented").and_then(|value| value.parse().ok());
    let bad_block_mode: Option<String> = request_header(&request, "x-bad-block-mode");
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let total = data.len();

    if s.serprog.is_some() && s.ch34x.is_none() {
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = (start_addr + total as u64) > 0x0100_0000;
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
                    data,
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
                    data,
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
                let bad_count = bad_blocks.len();
                let links = prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                protocols::nand_write(
                    &dev,
                    &params,
                    data,
                    info.size,
                    &op_bad,
                    mode,
                    force_segmented.unwrap_or(false),
                    &mut |done, total| {
                        app.emit("write_progress", WriteProgressEvent { done, total })
                            .ok();
                    },
                )?;
                if !links.is_empty() {
                    return Ok(format!(
                        "写入完成，共 {} 字节（已写入 {} 条 BBM 坏块映射）",
                        total,
                        links.len()
                    ));
                }
                if bad_count > 0 {
                    return Ok(format!(
                        "写入完成，共 {} 字节（发现 {} 个坏块，已跳过）",
                        total, bad_count
                    ));
                }
                return Ok(format!("写入完成，共 {} 字节", total));
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(&s, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::i2c_write(
                    &dev,
                    &params,
                    data,
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
                return protocols::mw_write(&dev, &params, data, start_addr, &mut |done, total| {
                    app.emit("write_progress", WriteProgressEvent { done, total })
                        .ok();
                })
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
            addr4b: (start_addr + total as u64) > 0x0100_0000,
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
async fn verify_chip(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let data = raw_request_bytes(&request)?;
    let start_addr: u64 = request_header(&request, "x-start-addr")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let bad_block_mode: Option<String> = request_header(&request, "x-bad-block-mode");
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let total = data.len() as u64;

    if s.serprog.is_some() && s.ch34x.is_none() {
        let ser = s.serprog.as_mut().unwrap();
        let use_4b = (start_addr + total) > 0x0100_0000;
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
                prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                protocols::nand_read(&dev, &params, total, &op_bad, &mut |_, _| {})?
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
            addr4b: (start_addr + total) > 0x0100_0000,
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
        dummy_mode: info.attr("dummyMode").map(|v| v.to_string()),
        read_mode: info.attr("readMode").map(|v| v.to_string()),
        write_mode: info.attr("writeMode").map(|v| v.to_string()),
        feature: info.attr_u32("feature"),
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
    // 并行 NAND 已入库，但当前所有编程器后端都尚未实现，UI 暂不显示。
    Ok(lib
        .list_protocols()
        .into_iter()
        .filter(|p| p != "PARALLEL_NAND")
        .collect())
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
struct ChipLibStatItem {
    protocol: String,
    count: usize,
}

#[derive(Serialize)]
struct ChipLibStats {
    total: usize,
    counts: Vec<ChipLibStatItem>,
}

#[tauri::command]
fn get_chip_lib_stats(state: State<'_, Mutex<AppState>>) -> Result<ChipLibStats, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let lib = get_lib(&s)?;
    let counts = lib
        .protocol_counts()
        .into_iter()
        .map(|(protocol, count)| ChipLibStatItem { protocol, count })
        .collect::<Vec<_>>();
    Ok(ChipLibStats {
        total: lib.entry_count(),
        counts,
    })
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("仅允许打开 http/https 链接".into());
    }
    open::that(&url).map_err(|e| format!("打开链接失败: {}", e))
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
async fn read_file(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|e| format!("读取文件失败 {}: {}", path, e))
}

/// Load a firmware image, decoding Intel HEX / S-record / UF2 containers
/// into plain bytes when needed. Raw images pass through unchanged.
#[tauri::command]
async fn load_firmware_file(path: String) -> Result<FirmwareLoadResult, String> {
    let (bytes, format) = firmware::load_firmware_file(&path)?;
    Ok(FirmwareLoadResult {
        length: bytes.len(),
        bytes,
        format: format.to_string(),
    })
}

#[tauri::command]
fn set_operation_running(state: State<'_, Mutex<AppState>>, running: bool) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.operation_running = running;
    Ok(())
}

#[tauri::command]
fn force_close_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("找不到主窗口")?;
    window.destroy().map_err(|e| e.to_string())
}

#[tauri::command]
fn write_file(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, &data).map_err(|e| format!("写入文件失败 {}: {}", path, e))
}

#[tauri::command]
fn load_settings() -> Result<String, String> {
    settings::load()
}

#[tauri::command]
fn save_settings(content: String) -> Result<String, String> {
    settings::save(&content)
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState {
            ch34x: None,
            serprog: None,
            lib: None,
            connected_device: None,
            detected: None,
            last_serial_ports: Vec::new(),
            cached_serprog: Vec::new(),
            operation_running: false,
        }))
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let busy = window
                    .state::<Mutex<AppState>>()
                    .lock()
                    .map(|s| s.operation_running)
                    .unwrap_or(false);
                if busy {
                    // Rust 侧同步拦截，保证任务栏右键“关闭窗口”也走确认流程。
                    api.prevent_close();
                    let _ = window.emit("close_requested_while_busy", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_chip_lib,
            initialize,
            connect_serprog,
            scan_programmers,
            detect_chip,
            scan_bad_blocks,
            read_nand_uid,
            read_nand_param_page,
            read_nand_bbm_lut,
            read_nand_otp_page,
            get_nand_ecc,
            set_nand_ecc,
            read_at45_page_mode,
            set_at45_page_mode,
            chip_erase,
            blank_check,
            read_chip,
            write_chip,
            verify_chip,
            get_chip_types,
            get_chip_vendors,
            get_chip_models,
            get_chip_info,
            get_chip_lib_stats,
            open_external_url,
            convert_chip_lib,
            import_chip_db,
            open_file_dialog,
            save_file_dialog,
            read_file,
            load_firmware_file,
            write_file,
            force_close_window,
            set_operation_running,
            load_settings,
            save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nor_4byte_boundary() {
        assert!(!nor_requires_4byte(0x0100_0000)); // exactly 16 MiB: 3-byte mode
        assert!(nor_requires_4byte(0x0100_0001)); // above 16 MiB: 4-byte mode
        assert!(nor_requires_4byte(0x0200_0000));
    }

    #[test]
    fn jedec_candidates_cover_shifted_nand_id() {
        let raw = [0xFF, 0x01, 0x25, 0xFF, 0xFF];
        let ids = jedec_id_candidates(&raw);
        assert!(ids.contains(&"0125".to_string()));
        assert!(ids.contains(&"FF0125".to_string()));
        assert!(ids.contains(&"0125FF".to_string()));
    }
}
