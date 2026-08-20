//! Integration test for `script_plugin::run_plugin_with_hal`: loads the mock
//! sidecar adapter plugin from a temporary plugin directory, starts the HAL
//! router and runs the `examples/script-protocol-plugin.js` script plugin
//! against the real child-process HAL path.

#[path = "../src/l0_core/script_plugin.rs"]
#[allow(dead_code)] // reusing the full module exposes more public API than this test needs
mod script_plugin;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use upt_hal::hal_router::HalRouter;
use upt_plugin::{self as plugin, PluginManager};

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
            "script_hal_plugin_test_{}_{}_{}",
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

const ADAPTER_MANIFEST: &str = r#"[package]
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

const PROTOCOL_MANIFEST: &str = r#"[package]
name = "vnd.test.protocol"
version = "1.0.0"
plugin_api = 1
kind = "protocol"
entry = "plugin.js"
"#;

#[test]
fn script_hal_plugin_reads_jedec_id_over_sidecar_hal() {
    let tmp = TempDir::new();
    let root = tmp.path();

    let plugin_dir = root.join("plugins").join("vnd.test.mock");
    fs::create_dir_all(&plugin_dir).expect("create plugin directory");

    fs::copy(
        env!("CARGO_BIN_EXE_sidecar_mock"),
        plugin_dir.join("mock.exe"),
    )
    .expect("copy mock sidecar binary into plugin directory");
    fs::write(plugin_dir.join("manifest.toml"), ADAPTER_MANIFEST).expect("write manifest");

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

    let manifest =
        plugin::PluginManifest::parse(PROTOCOL_MANIFEST).expect("protocol manifest should parse");

    let result = script_plugin::run_plugin_with_hal(
        &manifest,
        include_str!("../examples/script-protocol-plugin.js"),
        &mut router,
    )
    .expect("script plugin should run without errors");

    assert!(
        result
            .logs
            .iter()
            .any(|log| log.message == "script-protocol sees 1 adapter(s)"),
        "unexpected logs: {:?}",
        result.logs
    );
    assert!(
        result
            .logs
            .iter()
            .any(|log| log.message == "JEDEC ef 40 18"),
        "expected lowercase JEDEC log in: {:?}",
        result.logs
    );
    assert_eq!(result.registrations.len(), 1);
    assert_eq!(result.registrations[0].id, "vnd.example.sidecar-protocol");
    assert_eq!(result.registrations[0].kind, "protocol");
    assert_eq!(
        result.registrations[0].description.as_deref(),
        Some("Sidecar HAL example")
    );

    router.shutdown();
}
