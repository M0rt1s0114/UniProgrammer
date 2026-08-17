//! Programmer auto-detection.
//!
//! Produces a flat list of `ProgrammerCandidate`s from every transport we
//! support. USB enumeration is cheap and runs on every poll; serial-port
//! probing is more intrusive, so the caller decides when to run it (startup,
//! manual refresh, or when the port list changed).

use serde::Serialize;

#[cfg(any(hal_backend_dll, hal_backend_libusb))]
use crate::ch34x::ChipKind;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgrammerCandidate {
    /// Stable unique key for connection bookkeeping, e.g. `ch341:0`,
    /// `ch347f:1` or `serprog:COM3`.
    pub id: String,
    /// `ch341` | `ch347` | `ch347f` | `serprog`.
    pub kind: String,
    /// Human readable programmer name.
    pub name: String,
    /// Extra identification detail shown in the UI.
    pub detail: String,
    pub device_index: Option<u32>,
    pub usb_bus: Option<u8>,
    pub usb_address: Option<u8>,
    pub port: Option<String>,
}

fn kind_name(kind: ChipKind) -> (&'static str, &'static str) {
    match kind {
        ChipKind::Ch341A => ("ch341", "CH341A"),
        ChipKind::Ch347T => ("ch347", "CH347T"),
        ChipKind::Ch347F => ("ch347f", "CH347F"),
    }
}

/// Enumerate CH34X USB programmers. Exactly one HAL backend is compiled in.
pub fn scan_ch34x() -> Vec<ProgrammerCandidate> {
    let mut out = Vec::new();

    #[cfg(hal_backend_dll)]
    {
        match crate::ch34x::enumerate_dll_devices() {
            Ok(devices) => {
                for (index, kind) in devices {
                    let (kind_str, name) = kind_name(kind);
                    out.push(ProgrammerCandidate {
                        id: format!("{kind_str}:{index}"),
                        kind: kind_str.to_string(),
                        name: format!("{name} 编程器"),
                        detail: format!("CH34X 设备 {}", index),
                        device_index: Some(index),
                        usb_bus: None,
                        usb_address: None,
                        port: None,
                    });
                }
            }
            Err(e) => {
                eprintln!("[autodetect] CH34X DLL 枚举失败: {e}");
            }
        }
    }

    #[cfg(hal_backend_libusb)]
    {
        for (kind, bus, address) in crate::ch34x::enumerate_libusb_devices() {
            let (kind_str, name) = kind_name(kind);
            out.push(ProgrammerCandidate {
                id: format!("{kind_str}:{bus}:{address}"),
                kind: kind_str.to_string(),
                name: format!("{name} 编程器"),
                detail: format!("USB bus {} addr {}", bus, address),
                device_index: None,
                usb_bus: Some(bus),
                usb_address: Some(address),
                port: None,
            });
        }
    }

    out
}

/// Probe serial ports with the serprog handshake. Non-serprog ports are left
/// alone; `serprog::probe` uses a short timeout and no DTR reset.
pub fn scan_serprog(ports: &[String], quick: bool) -> Vec<ProgrammerCandidate> {
    let mut out = Vec::new();
    for port in ports {
        if let Some(version) = crate::serprog::Serprog::probe(port, quick) {
            out.push(ProgrammerCandidate {
                id: format!("serprog:{port}"),
                kind: "serprog".to_string(),
                name: format!("Serprog ({port})"),
                detail: format!("{} · {version}", port),
                device_index: None,
                usb_bus: None,
                usb_address: None,
                port: Some(port.clone()),
            });
        }
    }
    out
}
