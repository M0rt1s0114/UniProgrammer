//! Integration test for the sidecar-backed SPI NOR operation layer:
//! loads the mock sidecar adapter plugin from a temporary plugin directory,
//! starts the HAL router and drives erase / write / verify / read through
//! `operations::sidecar_*` over the real child-process protocol.

#[path = "../src/autodetect.rs"]
#[allow(dead_code)] // pulled in by core; only the sidecar path is exercised
mod autodetect;

#[path = "../src/core.rs"]
#[allow(dead_code)] // pulled in by operations; only the sidecar path is exercised
mod core;

use upt_proto::protocols;

#[path = "../src/operations.rs"]
#[allow(dead_code)] // reusing the full module exposes more public API than this test needs
mod operations;

#[allow(unused_imports)] // kept so the included app modules resolve the device transports
use upt_devices::{ch34x, serprog};

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use upt_chipdb as chiplib;
use upt_hal::hal_router::{HalRouter, SidecarSelection};
use upt_plugin::PluginManager;
use upt_proto::sfdp;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique temporary directory that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sidecar_operations_roundtrip_test_{}_{}_{}",
            std::process::id(),
            nanos,
            TMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("create temporary directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const MANIFEST: &str = r#"[package]
name = "vnd.test.mock"
version = "1.0.0"
plugin_api = 1
kind = "adapter"
entry = "mock.exe"

[capabilities.spi]
pins = { cs = "CS", sck = "SCK", mosi = "MOSI", miso = "MISO" }
max_frame = 4092
max_freq_khz = 60000
"#;

#[test]
fn sidecar_operations_spi_nor_roundtrip_over_sidecar_path() {
    let tmp = TempDir::new();
    let root = tmp.path();

    let plugin_dir = root.join("plugins").join("vnd.test.mock");
    fs::create_dir_all(&plugin_dir).expect("create plugin directory");

    fs::copy(
        env!("CARGO_BIN_EXE_sidecar_mock"),
        plugin_dir.join("mock.exe"),
    )
    .expect("copy mock sidecar binary into plugin directory");
    fs::write(plugin_dir.join("manifest.toml"), MANIFEST).expect("write manifest");

    let mut manager = PluginManager::load(root);
    manager
        .enable("vnd.test.mock")
        .expect("enable mock adapter");

    let mut router = HalRouter::start(&mut manager, root);
    assert_eq!(router.adapters.len(), 1, "one adapter must start");
    assert!(
        router.errors.is_empty(),
        "unexpected router start errors: {:?}",
        router.errors
    );

    let selection = SidecarSelection {
        adapter: "vnd.test.mock".into(),
        device_id: "mock-0".into(),
    };

    assert_eq!(
        operations::sidecar_erase_chip(&mut router, &selection).expect("erase chip"),
        "全片擦除完成（sidecar）"
    );

    let data = vec![0xA5u8; 512];
    assert_eq!(
        operations::sidecar_write_chip(&mut router, &selection, &data, 0, &mut |_, _| {})
            .expect("write chip"),
        "写入完成（sidecar），共 512 字节"
    );

    assert_eq!(
        operations::sidecar_verify_chip(&mut router, &selection, &data, 0, &mut |_, _| {})
            .expect("verify chip"),
        "校验通过（sidecar）"
    );

    let read_back = operations::sidecar_read_chip(&mut router, &selection, 512, &mut |_, _| {})
        .expect("read chip");
    assert_eq!(read_back, data);

    // Partial-page write must pad the tail of the last page with 0xFF, which
    // the mock's AND-program model leaves untouched after a fresh erase.
    assert_eq!(
        operations::sidecar_erase_chip(&mut router, &selection).expect("erase chip again"),
        "全片擦除完成（sidecar）"
    );

    let partial = vec![0xA5u8; 300];
    assert_eq!(
        operations::sidecar_write_chip(&mut router, &selection, &partial, 0, &mut |_, _| {})
            .expect("write partial chip"),
        "写入完成（sidecar），共 300 字节"
    );

    let read_back = operations::sidecar_read_chip(&mut router, &selection, 512, &mut |_, _| {})
        .expect("read back partial write");
    assert_eq!(&read_back[..300], &partial[..]);
    assert!(
        read_back[300..512].iter().all(|&byte| byte == 0xFF),
        "bytes 300..512 must remain erased (0xFF), got: {:02X?}",
        &read_back[300..512]
    );

    router.shutdown();
}
