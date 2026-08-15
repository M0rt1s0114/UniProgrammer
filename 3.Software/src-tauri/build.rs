use std::env;

fn main() {
    tauri_build::build();

    // ── HAL backend selection (build-menu equivalent for Cargo) ─────────────
    // hal-dll / hal-libusb features override the platform default.
    let dll = env::var_os("CARGO_FEATURE_HAL_DLL").is_some();
    let libusb = env::var_os("CARGO_FEATURE_HAL_LIBUSB").is_some();
    if dll && libusb {
        panic!("hal-dll 与 hal-libusb 互斥，不能同时启用");
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if dll && target_os != "windows" {
        panic!("hal-dll 后端仅在 Windows 可用");
    }
    let use_dll = dll || (!libusb && target_os == "windows");
    let use_libusb = libusb || (!dll && target_os != "windows");

    if use_dll {
        println!("cargo:rustc-cfg=hal_backend_dll");
    }
    if use_libusb {
        println!("cargo:rustc-cfg=hal_backend_libusb");
    }
    // 无论目标平台如何都声明这两个 cfg，避免未选中的后端触发 unexpected_cfgs
    println!("cargo:rustc-check-cfg=cfg(hal_backend_dll)");
    println!("cargo:rustc-check-cfg=cfg(hal_backend_libusb)");
}
