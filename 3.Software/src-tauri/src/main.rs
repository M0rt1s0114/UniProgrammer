#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autodetect;
mod ch34x;
mod chiplib;
mod core;
mod dialogs;
mod firmware;
pub mod hal_router;
pub mod nor_ops;
mod operations;
mod plugin;
mod protocols;
pub mod script_plugin;
mod serprog;
mod settings;
mod sfdp;
pub mod sidecar_nor;
pub mod spi_bus;
pub mod uni_hal;

use ch34x::{Ch34xDevice, Ch34xSettings, ChipKind};
use core::{
    chip_info_to_detect, get_lib, spi_read_jedec, AppState, At45PageModeResult, BadBlockScanResult,
    BbmLutResult, ChipDetectInfo, ChipDetectResult, NorWriteProtectStatus, RawBytesResult,
};
use hal_router::{HalRouter, SidecarSelection};
use operations::BlankCheckResult;
use plugin::{BuiltinModule, PluginManager};
use serde::Serialize;
use sidecar_nor::SidecarNor;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State, WindowEvent};

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
struct FirmwareLoadResult {
    length: usize,
    bytes: Vec<u8>,
    format: String,
}

#[derive(Serialize)]
struct SidecarAdapterEntry {
    name: String,
    devices: Vec<uni_hal::SidecarDevice>,
}

#[derive(Serialize)]
struct SidecarProbeResult {
    adapters: Vec<SidecarAdapterEntry>,
    errors: Vec<(String, String)>,
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
    core::detect_chip(&mut s)
}

#[tauri::command]
async fn nor_wp_status(state: State<'_, Mutex<AppState>>) -> Result<NorWriteProtectStatus, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::nor_wp_status(&mut s)
}

#[tauri::command]
async fn nor_wp_disable(state: State<'_, Mutex<AppState>>) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::nor_wp_disable(&mut s)
}

#[tauri::command]
async fn scan_bad_blocks(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<BadBlockScanResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::scan_bad_blocks(&mut s, &mut |done, total| {
        app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
            .ok();
    })
}

#[tauri::command]
fn read_nand_uid(state: State<'_, Mutex<AppState>>) -> Result<RawBytesResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::read_nand_uid(&mut s)
}

#[tauri::command]
fn read_nand_param_page(state: State<'_, Mutex<AppState>>) -> Result<RawBytesResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::read_nand_param_page(&mut s)
}

#[tauri::command]
fn read_nand_bbm_lut(state: State<'_, Mutex<AppState>>) -> Result<BbmLutResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::read_nand_bbm_lut(&mut s)
}

#[tauri::command]
fn read_nand_otp_page(
    state: State<'_, Mutex<AppState>>,
    page: u32,
) -> Result<RawBytesResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::read_nand_otp_page(&mut s, page)
}

#[tauri::command]
fn get_nand_ecc(state: State<'_, Mutex<AppState>>) -> Result<bool, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::get_nand_ecc(&mut s)
}

#[tauri::command]
fn set_nand_ecc(state: State<'_, Mutex<AppState>>, enable: bool) -> Result<bool, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::set_nand_ecc(&mut s, enable)
}

#[tauri::command]
fn read_at45_page_mode(state: State<'_, Mutex<AppState>>) -> Result<At45PageModeResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::read_at45_page_mode(&mut s)
}

#[tauri::command]
fn set_at45_page_mode(
    state: State<'_, Mutex<AppState>>,
    binary: bool,
) -> Result<At45PageModeResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    core::set_at45_page_mode(&mut s, binary)
}

#[tauri::command]
async fn chip_erase(
    state: State<'_, Mutex<AppState>>,
    router_state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
    bad_block_mode: Option<String>,
) -> Result<String, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut router = router_state.lock().map_err(|e| e.to_string())?;
    operations::chip_erase(
        &mut s,
        bad_block_mode.as_deref(),
        &mut |p| {
            app.emit(
                "erase_progress",
                EraseProgressEvent {
                    done: p.done,
                    total: p.total,
                    phase: p.phase,
                    message: p.message,
                    elapsed_ms: p.elapsed_ms,
                },
            )
            .ok();
        },
        &mut |done, total| {
            app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
                .ok();
        },
        Some(&mut router),
    )
}

/// Stream a blank check: read the requested range in chunks and stop at the
/// first non-0xFF byte. Never materialises the whole chip in memory.
#[tauri::command]
async fn blank_check(
    state: State<'_, Mutex<AppState>>,
    router_state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<String>,
) -> Result<BlankCheckResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut router = router_state.lock().map_err(|e| e.to_string())?;
    operations::blank_check(
        &mut s,
        size,
        start_addr,
        bad_block_mode.as_deref(),
        &mut |done, total| {
            app.emit(
                "blank_check_progress",
                BlankCheckProgressEvent { done, total },
            )
            .ok();
        },
        &mut |done, total| {
            app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
                .ok();
        },
        Some(&mut router),
    )
}

/// NOR read, IMSProg style: manual CS, 0x03 read, optional 4-byte address mode
/// (B7/E9 or Spansion BRWR), progress events.
#[tauri::command]
async fn read_chip(
    state: State<'_, Mutex<AppState>>,
    router_state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut router = router_state.lock().map_err(|e| e.to_string())?;
    let data = operations::read_chip(
        &mut s,
        size,
        start_addr,
        bad_block_mode.as_deref(),
        &mut |done, total| {
            app.emit("read_progress", ReadProgressEvent { done, total })
                .ok();
        },
        &mut |done, total| {
            app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
                .ok();
        },
        Some(&mut router),
    )?;
    Ok(tauri::ipc::Response::new(data))
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
    router_state: State<'_, Mutex<HalRouter>>,
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
    let mut router = router_state.lock().map_err(|e| e.to_string())?;
    operations::write_chip(
        &mut s,
        data,
        start_addr,
        force_segmented,
        bad_block_mode.as_deref(),
        &mut |done, total| {
            app.emit("write_progress", WriteProgressEvent { done, total })
                .ok();
        },
        &mut |done, total| {
            app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
                .ok();
        },
        Some(&mut router),
    )
}

#[tauri::command]
async fn verify_chip(
    state: State<'_, Mutex<AppState>>,
    router_state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let data = raw_request_bytes(&request)?;
    let start_addr: u64 = request_header(&request, "x-start-addr")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let bad_block_mode: Option<String> = request_header(&request, "x-bad-block-mode");
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut router = router_state.lock().map_err(|e| e.to_string())?;
    operations::verify_chip(
        &mut s,
        data,
        start_addr,
        bad_block_mode.as_deref(),
        &mut |done, total| {
            app.emit("verify_progress", VerifyProgressEvent { done, total })
                .ok();
        },
        &mut |done, total| {
            app.emit("bad_block_progress", BadBlockProgressEvent { done, total })
                .ok();
        },
        Some(&mut router),
    )
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

#[derive(Serialize)]
struct PluginListEntry {
    name: String,
    version: String,
    kind: String,
    enabled: bool,
    error: Option<String>,
}

#[tauri::command]
fn plugin_list(state: State<'_, Mutex<PluginManager>>) -> Result<Vec<PluginListEntry>, String> {
    let manager = state.lock().map_err(|e| e.to_string())?;
    Ok(manager
        .plugins
        .iter()
        .map(|p| {
            let manifest_path = p.path.join("manifest.toml").display().to_string();
            let error = manager
                .errors
                .iter()
                .find(|(key, _)| *key == p.manifest.name || *key == manifest_path)
                .map(|(_, e)| e.clone());
            PluginListEntry {
                name: p.manifest.name.clone(),
                version: p.manifest.version.to_string(),
                kind: p.manifest.kind.as_str().to_string(),
                enabled: p.enabled,
                error,
            }
        })
        .collect())
}

#[tauri::command]
fn plugin_builtin_modules() -> Result<Vec<BuiltinModule>, String> {
    Ok(plugin::builtin_modules())
}

#[tauri::command]
fn plugin_enable(state: State<'_, Mutex<PluginManager>>, name: String) -> Result<String, String> {
    let mut manager = state.lock().map_err(|e| e.to_string())?;
    manager.enable(&name)?;
    Ok(format!("plugin enabled: {name}"))
}

#[tauri::command]
fn plugin_disable(state: State<'_, Mutex<PluginManager>>, name: String) -> Result<String, String> {
    let mut manager = state.lock().map_err(|e| e.to_string())?;
    manager.disable(&name)?;
    Ok(format!("plugin disabled: {name}"))
}

#[tauri::command]
fn sidecar_adapters(
    state: State<'_, Mutex<HalRouter>>,
) -> Result<Vec<SidecarAdapterEntry>, String> {
    let router = state.lock().map_err(|e| e.to_string())?;
    let probe_result = SidecarProbeResult {
        adapters: router
            .adapters
            .iter()
            .map(|adapter| SidecarAdapterEntry {
                name: adapter.name.clone(),
                devices: adapter.devices.clone(),
            })
            .collect(),
        errors: router.errors.clone(),
    };
    Ok(probe_result.adapters)
}

#[tauri::command]
fn sidecar_open(
    state: State<'_, Mutex<HalRouter>>,
    adapter: String,
    device: String,
) -> Result<String, String> {
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let selection = SidecarSelection {
        adapter,
        device_id: device,
    };
    router.open(&selection.adapter, &selection.device_id)
}

#[tauri::command]
fn sidecar_select(
    state: State<'_, Mutex<AppState>>,
    router_state: State<'_, Mutex<HalRouter>>,
    adapter: String,
    device: String,
) -> Result<String, String> {
    let session_id = {
        let mut router = router_state.lock().map_err(|e| e.to_string())?;
        router.open(&adapter, &device)?
    };
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.sidecar_adapter = Some(adapter);
    s.sidecar_device = Some(device);
    Ok(session_id)
}

#[tauri::command]
fn sidecar_unselect(
    state: State<'_, Mutex<AppState>>,
    router_state: State<'_, Mutex<HalRouter>>,
) -> Result<(), String> {
    let previous = {
        let mut s = state.lock().map_err(|e| e.to_string())?;
        (s.sidecar_adapter.take(), s.sidecar_device.take())
    };
    if let (Some(adapter), Some(device)) = previous {
        if let Ok(mut router) = router_state.lock() {
            let _ = router.close(&adapter, &device);
        }
    }
    Ok(())
}

#[tauri::command]
fn sidecar_close(
    state: State<'_, Mutex<HalRouter>>,
    adapter: String,
    device: String,
) -> Result<String, String> {
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let selection = SidecarSelection {
        adapter,
        device_id: device,
    };
    router.close(&selection.adapter, &selection.device_id)?;
    Ok(format!(
        "closed {} / {}",
        selection.adapter, selection.device_id
    ))
}

#[tauri::command]
fn sidecar_read_id(
    state: State<'_, Mutex<HalRouter>>,
    adapter: String,
    device: String,
) -> Result<String, String> {
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let mut nor = SidecarNor::open(&mut router, &adapter, &device)?;
    let id = nor.read_id()?;
    nor.close()?;
    Ok(format!(
        "JEDEC ID: {:02X} {:02X} {:02X}",
        id[0], id[1], id[2]
    ))
}

/// Debug/advanced command: run a raw full-duplex SPI transaction against a
/// sidecar adapter device. The session is opened when needed and then kept
/// open for subsequent calls.
#[tauri::command]
fn sidecar_spi_transact(
    state: State<'_, Mutex<HalRouter>>,
    adapter: String,
    device: String,
    write: Vec<u8>,
    read_len: usize,
) -> Result<String, String> {
    let mut router = state.lock().map_err(|e| e.to_string())?;
    router.open(&adapter, &device)?;
    let data = router.spi_transact(&adapter, &device, &write, read_len)?;
    Ok(data
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" "))
}

#[tauri::command]
fn sidecar_errors(state: State<'_, Mutex<HalRouter>>) -> Result<Vec<(String, String)>, String> {
    let router = state.lock().map_err(|e| e.to_string())?;
    Ok(router.errors.clone())
}

#[tauri::command]
fn sidecar_shutdown(state: State<'_, Mutex<HalRouter>>) -> Result<(), String> {
    let mut router = state.lock().map_err(|e| e.to_string())?;
    router.shutdown();
    Ok(())
}

/// 通过 sidecar 插件适配器执行整片擦除。
#[tauri::command]
fn sidecar_erase(
    adapter: String,
    device: String,
    state: State<'_, Mutex<HalRouter>>,
) -> Result<String, String> {
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let selection = SidecarSelection {
        adapter,
        device_id: device,
    };
    operations::sidecar_erase_chip(&mut router, &selection)
}

/// 通过 sidecar 插件适配器读取 NOR 数据，并通过 read_progress 事件上报进度。
#[tauri::command]
fn sidecar_read(
    adapter: String,
    device: String,
    size: u64,
    start_addr: Option<u64>,
    state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
) -> Result<tauri::ipc::Response, String> {
    // sidecar_read_chip 暂不支持起始地址，保留该参数供后续扩展。
    let _ = start_addr;
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let selection = SidecarSelection {
        adapter,
        device_id: device,
    };
    let data = operations::sidecar_read_chip(&mut router, &selection, size, &mut |done, total| {
        let _ = app.emit("read_progress", ReadProgressEvent { done, total });
    })?;
    Ok(tauri::ipc::Response::new(data))
}

/// 通过 sidecar 插件适配器写入 NOR 数据，并通过 write_progress 事件上报进度。
#[tauri::command]
fn sidecar_write(
    request: tauri::ipc::Request<'_>,
    state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let adapter =
        request_header(&request, "x-adapter").ok_or_else(|| "缺少 x-adapter 请求头".to_string())?;
    let device =
        request_header(&request, "x-device").ok_or_else(|| "缺少 x-device 请求头".to_string())?;
    let start_addr: u64 = request_header(&request, "x-start-addr")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let data = raw_request_bytes(&request)?;
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let selection = SidecarSelection {
        adapter,
        device_id: device,
    };
    operations::sidecar_write_chip(
        &mut router,
        &selection,
        data,
        start_addr,
        &mut |done, total| {
            let _ = app.emit("write_progress", WriteProgressEvent { done, total });
        },
    )
}

/// 通过 sidecar 插件适配器校验 NOR 数据，并通过 verify_progress 事件上报进度。
#[tauri::command]
fn sidecar_verify(
    request: tauri::ipc::Request<'_>,
    state: State<'_, Mutex<HalRouter>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let adapter =
        request_header(&request, "x-adapter").ok_or_else(|| "缺少 x-adapter 请求头".to_string())?;
    let device =
        request_header(&request, "x-device").ok_or_else(|| "缺少 x-device 请求头".to_string())?;
    let start_addr: u64 = request_header(&request, "x-start-addr")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let data = raw_request_bytes(&request)?;
    let mut router = state.lock().map_err(|e| e.to_string())?;
    let selection = SidecarSelection {
        adapter,
        device_id: device,
    };
    operations::sidecar_verify_chip(
        &mut router,
        &selection,
        data,
        start_addr,
        &mut |done, total| {
            let _ = app.emit("verify_progress", VerifyProgressEvent { done, total });
        },
    )
}

fn main() {
    let exe = exe_dir();
    let mut plugin_manager = PluginManager::load(&exe);
    let hal_router = HalRouter::start(&mut plugin_manager, &exe);
    tauri::Builder::default()
        .manage(Mutex::new(AppState {
            ch34x: None,
            serprog: None,
            lib: None,
            connected_device: None,
            detected: None,
            sidecar_adapter: None,
            sidecar_device: None,
            last_serial_ports: Vec::new(),
            cached_serprog: Vec::new(),
            operation_running: false,
        }))
        .manage(Mutex::new(plugin_manager))
        .manage(Mutex::new(hal_router))
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
            nor_wp_status,
            nor_wp_disable,
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
            plugin_list,
            plugin_builtin_modules,
            plugin_enable,
            plugin_disable,
            sidecar_adapters,
            sidecar_open,
            sidecar_select,
            sidecar_unselect,
            sidecar_close,
            sidecar_read_id,
            sidecar_spi_transact,
            sidecar_errors,
            sidecar_shutdown,
            sidecar_erase,
            sidecar_read,
            sidecar_write,
            sidecar_verify,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nor_4byte_boundary() {
        assert!(!core::nor_requires_4byte(0x0100_0000)); // exactly 16 MiB: 3-byte mode
        assert!(core::nor_requires_4byte(0x0100_0001)); // above 16 MiB: 4-byte mode
        assert!(core::nor_requires_4byte(0x0200_0000));
    }

    #[test]
    fn jedec_candidates_cover_shifted_nand_id() {
        let raw = [0xFF, 0x01, 0x25, 0xFF, 0xFF];
        let ids = core::jedec_id_candidates(&raw);
        assert!(ids.contains(&"0125".to_string()));
        assert!(ids.contains(&"FF0125".to_string()));
        assert!(ids.contains(&"0125FF".to_string()));
    }
}
