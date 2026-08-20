#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app_ops;
pub mod l0_core;
pub mod ui_tauri;

use app_ops::core;
use l0_core::{console, runtime, settings, unipkg_protocol, upt_log};
use std::path::Path;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WindowEvent};
use upt_hal::hal_router::HalRouter;
use upt_plugin::{BootCheck, PluginManager};

/// Write a readable Chinese boot-error report next to the executable and stop
/// the process when the required L1 plugin set is not healthy.
fn write_boot_error(base: &Path, boot: &BootCheck) {
    let mut message = String::from("UniProgrammer 启动失败：必需插件缺失或无效\n\n");

    if !boot.missing.is_empty() {
        message.push_str("缺少的必需插件:\n");
        for name in &boot.missing {
            message.push_str(&format!("  - {name}\n"));
        }
        message.push('\n');
    }

    if !boot.invalid.is_empty() {
        message.push_str("无效的必需插件:\n");
        for name in &boot.invalid {
            message.push_str(&format!("  - {name}\n"));
        }
        message.push('\n');
    }

    message.push_str("请恢复 plugins/builtin 目录下的内置插件清单后重试。\n");

    let error_path = base.join("uniprog-boot-error.txt");
    if let Err(e) = std::fs::write(&error_path, message.as_bytes()) {
        eprintln!("写入启动错误文件失败 {}: {e}", error_path.display());
    }
}

fn main() {
    let debug_console = settings::startup_debug_console();
    let log_level = if debug_console {
        upt_log::Level::Debug
    } else {
        upt_log::Level::Info
    };
    // 先初始化文本日志，再分配控制台；文件 sink 始终开启。调试构建或
    // 用户开启“调试控制台”时，同样把日志写到 stderr。
    let _ = upt_log::init(
        log_level,
        debug_console || cfg!(debug_assertions),
        Some(settings::log_file()),
    );

    if debug_console {
        // 必须在任何 eprintln!/侧车进程启动之前完成，这样后端日志和
        // CH34X 侧车的 stderr 都会进入同一个控制台窗口。
        console::attach();
        std::env::set_var("UNIPROG_DEBUG_CONSOLE", "1");
        runtime::log_info("调试控制台已启用");
    }

    let exe = runtime::exe_dir();
    runtime::log_info(&format!("UniProgrammer 启动，根目录: {}", exe.display()));
    let mut plugin_manager = PluginManager::load(&exe);
    let boot = plugin_manager.boot_check();
    if !boot.missing.is_empty() || !boot.invalid.is_empty() {
        runtime::log_info("启动失败：L1 必需插件缺失或无效");
        write_boot_error(&exe, &boot);
        std::process::exit(1);
    }
    runtime::log_info(&format!(
        "插件扫描完成：{} 个插件，{} 个错误",
        plugin_manager.plugins.len(),
        plugin_manager.errors.len()
    ));
    let hal_router = HalRouter::start(&mut plugin_manager, &exe);
    let plugin_assets = unipkg_protocol::UnipkgProtocol::from_manager(&plugin_manager);
    let builder = tauri::Builder::default()
        .manage(Mutex::new(core::AppState {
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
                    .state::<Mutex<core::AppState>>()
                    .lock()
                    .map(|s| s.operation_running)
                    .unwrap_or(false);
                if busy {
                    // Rust 侧同步拦截，保证任务栏右键“关闭窗口”也走确认流程。
                    api.prevent_close();
                    let _ = window.emit("close_requested_while_busy", ());
                }
            }
        });
    let builder = ui_tauri::commands::attach_handler(builder);

    unipkg_protocol::UnipkgProtocol::register(builder, plugin_assets)
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
