//! Debug console support (Windows).
//!
//! The main executable is a GUI-subsystem binary, so it normally never
//! allocates a console. When the user enables “调试控制台” in settings,
//! [`attach`] allocates one very early in `main` and routes Rust's standard
//! error output to it; sidecar child processes inherit the same console when
//! `UNIPROG_DEBUG_CONSOLE=1` is set (see `uni-hal`).

/// Allocate a console and redirect the standard output/error handles to it.
#[cfg(windows)]
pub fn attach() {
    use windows::Win32::System::Console::{
        AllocConsole, GetStdHandle, SetConsoleOutputCP, SetStdHandle, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    unsafe {
        if AllocConsole().is_err() {
            // A console may already be attached (for example when launched
            // from an existing terminal); still refresh the handles below.
        }
        if let Ok(stdout) = GetStdHandle(STD_OUTPUT_HANDLE) {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, stdout);
        }
        if let Ok(stderr) = GetStdHandle(STD_ERROR_HANDLE) {
            let _ = SetStdHandle(STD_ERROR_HANDLE, stderr);
        }
        // UTF-8 so Chinese log lines render correctly on modern consoles.
        let _ = SetConsoleOutputCP(65001);
    }
}

/// No-op on platforms without a Windows console.
#[cfg(not(windows))]
pub fn attach() {}
