//! Integration test for `ChildTransport` / `spawn_sidecar`: spawns the mock
//! sidecar binary as a real child process and drives a full SPI session over
//! the framed stdio protocol.

#[path = "../src/uni_hal.rs"]
#[allow(dead_code)] // only the child-process path is exercised by this integration test
mod uni_hal;

#[path = "../src/plugin.rs"]
#[allow(dead_code)] // reusing the full module exposes more public API than this test needs
mod plugin;

use plugin::{CapabilitySet, SpiCapability};
use serde_json::json;

fn spi_capabilities() -> CapabilitySet {
    CapabilitySet {
        spi: Some(SpiCapability {
            pins: Some((
                "CS0".to_string(),
                "SCK".to_string(),
                "MOSI".to_string(),
                "MISO".to_string(),
            )),
            max_frame: 4092,
            max_freq_khz: 60000,
        }),
        ..CapabilitySet::default()
    }
}

#[test]
fn sidecar_process_spi_roundtrip() {
    let caps = spi_capabilities();

    let mut client = uni_hal::spawn_sidecar(
        env!("CARGO_BIN_EXE_sidecar_mock"),
        &[],
        "uni-hal-integration-test",
        "1.0.0",
        &caps,
    )
    .expect("spawn_sidecar should spawn and handshake with the mock");

    let devices = client.probe().expect("probe should succeed");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, "mock-0");
    assert_eq!(devices[0].kind, "spi");

    let session = client.open("mock-0").expect("open should succeed");
    assert_eq!(session, "mock-session-1");

    let jedec = client
        .spi_transact(&session, &[0x9F], 3)
        .expect("spi_transact should succeed");
    assert_eq!(jedec, vec![0xEF, 0x40, 0x18]);

    let err = client
        .execute(
            &session,
            &json!({ "op": "gpio_set", "pin": "IO0", "level": true }),
        )
        .expect_err("gpio_set must be rejected because the mock only declares SPI");
    assert!(err.contains("CAPABILITY_NOT_EXPOSED"), "{err}");

    client.close(&session).expect("close should succeed");

    drop(client);
}
