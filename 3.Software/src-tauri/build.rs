use std::env;

fn main() {
    // CH34X.DLL 是可选资源：本地 Windows 打包需要它，但仓库/CI 中没有它。
    // 不存在时通过 TAURI_CONFIG 把它从 bundle.resources 里去掉，否则
    // tauri-build 会因资源路径不存在而失败。
    let has_dll = std::path::Path::new("CH34X.DLL").exists();
    if !has_dll {
        let override_config = r#"{"bundle":{"resources":["chiplib.bin"]}}"#;
        env::set_var("TAURI_CONFIG", override_config);
        println!("cargo:rustc-env=TAURI_CONFIG={}", override_config);
    }
    println!("cargo:rerun-if-changed=CH34X.DLL");

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
