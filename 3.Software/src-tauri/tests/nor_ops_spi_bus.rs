//! Integration test for `nor_ops` over `spi_bus::SidecarSpiBus`: loads the
//! mock sidecar adapter plugin from a temporary plugin directory, starts the
//! HAL router and drives the transport-agnostic SPI NOR command primitives
//! over the real child-process protocol.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uni_hal::hal_router::HalRouter;
use uni_hal::nor_ops;
use uni_hal::spi_bus::SidecarSpiBus;
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
            "nor_ops_spi_bus_test_{}_{}_{}",
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
fn nor_ops_spi_nor_roundtrip_over_sidecar_bus() {
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

    assert_eq!(
        nor_ops::read_id(&mut bus).expect("read JEDEC ID"),
        [0xEF, 0x40, 0x18]
    );

    nor_ops::erase_chip(&mut bus).expect("erase chip");

    let page_a = vec![0xA5u8; 256];
    nor_ops::page_program(&mut bus, 0, &page_a).expect("program page at addr 0");
    nor_ops::verify(&mut bus, 0, &page_a).expect("verify page at addr 0");

    let page_b = vec![0xA5u8; 100];
    nor_ops::page_program(&mut bus, 4096, &page_b).expect("program page at addr 4096");
    nor_ops::verify(&mut bus, 4096, &page_b).expect("verify page at addr 4096");

    let first_bytes = nor_ops::read(&mut bus, 0, 4).expect("read first bytes");
    assert_eq!(first_bytes, vec![0xA5, 0xA5, 0xA5, 0xA5]);

    let page_b_head = nor_ops::read(&mut bus, 4096, 4).expect("read second region first bytes");
    assert_eq!(page_b_head, vec![0xA5, 0xA5, 0xA5, 0xA5]);

    router.shutdown();
}
