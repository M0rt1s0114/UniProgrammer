//! Integration test for `sidecar_nor`: loads the mock sidecar adapter plugin
//! from a temporary plugin directory, starts the HAL router and drives a full
//! SPI NOR read / erase / program / verify cycle over the real child-process
//! protocol.

#[path = "../src/uni_hal.rs"]
#[allow(dead_code)] // only the child-process path is exercised by this integration test
mod uni_hal;

#[path = "../src/plugin.rs"]
#[allow(dead_code)] // reusing the full module exposes more public API than this test needs
mod plugin;

#[path = "../src/hal_router.rs"]
#[allow(dead_code)] // reusing the full module exposes more public API than this test needs
mod hal_router;

#[path = "../src/sidecar_nor.rs"]
#[allow(dead_code)] // reusing the full module exposes more public API than this test needs
mod sidecar_nor;

use hal_router::HalRouter;
use plugin::PluginManager;
use sidecar_nor::SidecarNor;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
            "sidecar_nor_roundtrip_test_{}_{}_{}",
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
fn sidecar_nor_spi_nor_roundtrip_over_sidecar_path() {
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

    let mut nor =
        SidecarNor::open(&mut router, "vnd.test.mock", "mock-0").expect("open SPI NOR sidecar");

    assert_eq!(nor.read_id().expect("read JEDEC ID"), [0xEF, 0x40, 0x18]);

    nor.erase_chip().expect("erase chip");

    let page_a = vec![0xA5u8; 256];
    nor.program_page(0, &page_a)
        .expect("program page at addr 0");
    nor.verify(0, &page_a).expect("verify page at addr 0");

    let page_b = vec![0xA5u8; 100];
    nor.program_page(4096, &page_b)
        .expect("program page at addr 4096");
    nor.verify(4096, &page_b).expect("verify page at addr 4096");

    // Reprogramming with 0x0F over 0xA5 must AND into the flash model.
    nor.program_page(0, &[0x0F]).expect("reprogram first byte");
    let read_back = nor.read(0, 1).expect("read back first byte");
    assert_eq!(read_back, vec![0x05]);

    nor.close().expect("close SPI NOR sidecar");
    router.shutdown();
}
