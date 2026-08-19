//! Integration test for `spi_bus::SidecarSpiBus`: loads the mock sidecar
//! adapter plugin from a temporary plugin directory, starts the HAL router
//! and drives single-shot SPI bus transactions over the real child-process
//! protocol.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uni_hal::hal_router::HalRouter;
use uni_hal::spi_bus::{SidecarSpiBus, SpiBus};
use uni_plugin::PluginManager;

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
            "spi_bus_sidecar_test_{}_{}_{}",
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
fn sidecar_spi_bus_transacts_over_sidecar_path() {
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

    let mut bus = SidecarSpiBus {
        router: &mut router,
        adapter: "vnd.test.mock".to_string(),
        device_id: "mock-0".to_string(),
    };

    assert_eq!(bus.max_write(), 4096);
    assert_eq!(bus.max_read(), 65536);

    let jedec = bus.transact(&[0x9F], 3).expect("read JEDEC ID");
    assert_eq!(jedec, vec![0xEF, 0x40, 0x18]);

    bus.transact(&[0x06], 0).expect("write enable");
    bus.transact(&[0xC7], 0).expect("erase chip");

    let erased = bus
        .transact(&[0x03, 0, 0, 0], 3)
        .expect("read first three bytes after chip erase");
    assert_eq!(erased, vec![0xFF, 0xFF, 0xFF]);

    router.shutdown();
}
