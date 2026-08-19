//! Integration test for `hal_router`: loads a mock sidecar adapter plugin from
//! a temporary plugin directory, starts the HAL router and drives a full
//! open -> SPI transaction -> close -> shutdown cycle over the real child
//! process protocol.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uni_hal::hal_router::HalRouter;
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
            "hal_router_process_test_{}_{}_{}",
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
fn hal_router_routes_sidecar_adapter_plugin() {
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
    assert_eq!(router.adapters[0].name, "vnd.test.mock");
    assert_eq!(router.adapters[0].path, plugin_dir.join("mock.exe"));

    let devices = router.adapter_devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].0, "vnd.test.mock");
    assert_eq!(devices[0].1.len(), 1);
    assert_eq!(devices[0].1[0].id, "mock-0");
    assert_eq!(devices[0].1[0].kind, "spi");

    let session_id = router
        .open("vnd.test.mock", "mock-0")
        .expect("open mock device session");
    assert_eq!(session_id, "mock-session-1");

    let jedec = router
        .spi_transact("vnd.test.mock", "mock-0", &[0x9F], 3)
        .expect("SPI transaction should succeed");
    assert_eq!(jedec, vec![0xEF, 0x40, 0x18]);

    router
        .close("vnd.test.mock", "mock-0")
        .expect("close mock device session");
    router.shutdown();
}
