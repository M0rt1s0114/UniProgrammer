//! L0 runtime helpers shared by startup and the UI command layer.

use std::path::PathBuf;

/// Application root directory:
/// - debug: the process working directory;
/// - release: the executable directory (portable layout).
pub fn exe_dir() -> PathBuf {
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

/// Info-level message through the process-wide `upt.log` pipeline.
pub fn log_info(message: &str) {
    crate::l0_core::upt_log::info(message);
}
