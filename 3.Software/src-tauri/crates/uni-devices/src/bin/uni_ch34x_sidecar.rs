//! CH34X sidecar adapter process for the v1 sidecar protocol.
//!
//! It speaks framed JSON-RPC on stdin/stdout with the same 4-byte
//! little-endian length + UTF-8 JSON framing as the serprog sidecar.
//! Debug logs go to stderr only; stdout is reserved for protocol frames.
//!
//! The process is meant to be spawned later as a cold-start plugin by the
//! uni-hal sidecar transport, so it only depends on the `uni-devices` crate
//! and never on the app crate.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

use uni_devices::ch34x::{self, Ch34xDevice, Ch34xSettings, ChipKind, DeviceMode};

const FRAME_HEADER_LEN: usize = 4;
const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

const CAPABILITY_NOT_EXPOSED: i32 = -32001;
const BUSY: i32 = -32002;
const INVALID_PARAMS: i32 = -32602;
const METHOD_NOT_FOUND: i32 = -32601;
const SERVER_ERROR: i32 = -32000;

// ---------------------------------------------------------------------------
// Frame helpers (same wire format as examples/serprog_sidecar.rs).
// ---------------------------------------------------------------------------

fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, String> {
    let len = u32::try_from(payload.len())
        .map_err(|_| format!("payload too large to frame: {} bytes", payload.len()))?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + len as usize);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), String> {
    let frame = encode_frame(payload)?;
    writer
        .write_all(&frame)
        .map_err(|e| format!("failed to write frame: {e}"))
}

fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut header = [0u8; FRAME_HEADER_LEN];
    reader
        .read_exact(&mut header)
        .map_err(|e| format!("failed to read 4-byte frame length header: {e}"))?;

    let payload_len = u32::from_le_bytes(header) as usize;
    if payload_len > MAX_FRAME_LEN {
        return Err(format!(
            "declared frame length {payload_len} exceeds maximum {MAX_FRAME_LEN}"
        ));
    }

    let mut payload = vec![0u8; payload_len];
    reader
        .read_exact(&mut payload)
        .map_err(|e| format!("failed to read frame payload of {payload_len} bytes: {e}"))?;
    Ok(payload)
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers.
// ---------------------------------------------------------------------------

fn success_response(id: u64, result: Value) -> Value {
    json!({ "id": id, "result": result })
}

fn error_response(id: u64, code: i32, message: impl Into<String>) -> Value {
    json!({ "id": id, "error": { "code": code, "message": message.into() } })
}

// ---------------------------------------------------------------------------
// Device enumeration / open parsing.
// ---------------------------------------------------------------------------

fn kind_wire(kind: ChipKind) -> (&'static str, &'static str) {
    match kind {
        ChipKind::Ch341A => ("ch341", "CH341A"),
        ChipKind::Ch347T => ("ch347", "CH347T"),
        ChipKind::Ch347F => ("ch347f", "CH347F"),
    }
}

/// A CH34X device id has one of these forms:
/// `ch34x:<kind>:<dll-index>` or `ch34x:<kind>:<bus>:<address>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceLocation {
    DllIndex(u32),
    UsbBusAddr { bus: u8, address: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedDeviceId {
    kind: ChipKind,
    location: DeviceLocation,
}

fn parse_ch34x_device_id(device_id: &str) -> Result<ParsedDeviceId, String> {
    let rest = device_id
        .strip_prefix("ch34x:")
        .ok_or_else(|| format!("device_id must start with 'ch34x:': {device_id}"))?;

    let mut parts = rest.split(':');
    let kind_str = parts.next().unwrap_or("");
    let kind = match kind_str {
        "ch341" => ChipKind::Ch341A,
        "ch347" => ChipKind::Ch347T,
        "ch347f" => ChipKind::Ch347F,
        other => return Err(format!("unknown CH34X kind '{other}' in device_id")),
    };

    let first = parts
        .next()
        .ok_or_else(|| "device_id must include an index or bus:address".to_string())?;
    let second = parts.next();
    let third = parts.next();
    if third.is_some() {
        return Err("device_id has too many ':'-separated parts".to_string());
    }

    match second {
        Some(addr) => {
            let bus = first
                .parse::<u8>()
                .map_err(|e| format!("invalid USB bus '{first}': {e}"))?;
            let address = addr
                .parse::<u8>()
                .map_err(|e| format!("invalid USB address '{addr}': {e}"))?;
            Ok(ParsedDeviceId {
                kind,
                location: DeviceLocation::UsbBusAddr { bus, address },
            })
        }
        None => {
            let index = first
                .parse::<u32>()
                .map_err(|e| format!("invalid DLL device index '{first}': {e}"))?;
            Ok(ParsedDeviceId {
                kind,
                location: DeviceLocation::DllIndex(index),
            })
        }
    }
}

fn settings_for(parsed: &ParsedDeviceId) -> Ch34xSettings {
    match parsed.location {
        DeviceLocation::DllIndex(index) => Ch34xSettings {
            kind: parsed.kind,
            device_index: index,
            ..Ch34xSettings::default()
        },
        DeviceLocation::UsbBusAddr { bus, address } => Ch34xSettings {
            kind: parsed.kind,
            usb_bus: Some(bus),
            usb_address: Some(address),
            ..Ch34xSettings::default()
        },
    }
}

fn device_json(id: String, kind: &str, detail: String) -> Value {
    json!({ "id": id, "kind": kind, "detail": detail })
}

#[cfg(hal_backend_dll)]
fn probe_devices() -> Vec<Value> {
    match ch34x::DllHal::enumerate() {
        Ok(devices) => devices
            .into_iter()
            .map(|(index, kind)| {
                let (wire, label) = kind_wire(kind);
                device_json(
                    format!("ch34x:{wire}:{index}"),
                    wire,
                    format!("{label} at DLL index {index}"),
                )
            })
            .collect(),
        Err(e) => {
            eprintln!("[uni_ch34x_sidecar] probe enumeration error: {e}");
            Vec::new()
        }
    }
}

#[cfg(hal_backend_libusb)]
fn probe_devices() -> Vec<Value> {
    if let Err(e) = rusb::devices() {
        eprintln!("[uni_ch34x_sidecar] probe enumeration error: {e}");
        return Vec::new();
    }

    ch34x::enumerate_libusb_devices()
        .into_iter()
        .map(|(kind, bus, address)| {
            let (wire, label) = kind_wire(kind);
            device_json(
                format!("ch34x:{wire}:{bus}:{address}"),
                wire,
                format!("{label} at USB bus {bus} address {address}"),
            )
        })
        .collect()
}

#[cfg(not(any(hal_backend_dll, hal_backend_libusb)))]
fn probe_devices() -> Vec<Value> {
    eprintln!("[uni_ch34x_sidecar] probe enumeration unavailable: no CH34X HAL backend configured");
    Vec::new()
}

fn handle_open(
    id: u64,
    params: &Value,
    session: &mut Option<Session>,
    next_session_id: &mut u64,
) -> Value {
    let device_id = params
        .get("device_id")
        .and_then(Value::as_str)
        .unwrap_or("?");
    eprintln!("[uni_ch34x_sidecar] open device={device_id}");

    let parsed = match parse_ch34x_device_id(device_id) {
        Ok(parsed) => parsed,
        Err(e) => {
            let message = format!("invalid device_id: {e}");
            eprintln!("[uni_ch34x_sidecar] {message}");
            return error_response(id, INVALID_PARAMS, message);
        }
    };

    let settings = settings_for(&parsed);
    match Ch34xDevice::open_with_mode(&settings, DeviceMode::Spi) {
        Ok(device) => {
            let sid = format!("ch34x-session-{next_session_id}");
            *next_session_id += 1;
            *session = Some(Session {
                id: sid.clone(),
                device,
            });
            success_response(id, json!({ "session_id": sid }))
        }
        Err(e) => error_response(id, SERVER_ERROR, format!("failed to open {device_id}: {e}")),
    }
}

// ---------------------------------------------------------------------------
// execute helpers.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct SpiTransact {
    write_bytes: Vec<u8>,
    read_len: usize,
}

fn decode_spi_transact(op: &Value) -> Result<SpiTransact, (i32, String)> {
    let Some(write_b64) = op.get("write_b64") else {
        return Err((INVALID_PARAMS, "missing write_b64".to_string()));
    };
    let Some(write_b64) = write_b64.as_str() else {
        return Err((INVALID_PARAMS, "write_b64 must be a string".to_string()));
    };

    let Some(read_len) = op.get("read_len") else {
        return Err((INVALID_PARAMS, "missing read_len".to_string()));
    };
    let Some(read_len) = read_len.as_u64() else {
        return Err((
            INVALID_PARAMS,
            "read_len must be an unsigned integer".to_string(),
        ));
    };
    let Ok(read_len) = usize::try_from(read_len) else {
        return Err((INVALID_PARAMS, "read_len does not fit usize".to_string()));
    };

    let write_bytes = STANDARD
        .decode(write_b64)
        .map_err(|e| (INVALID_PARAMS, format!("invalid write_b64: {e}")))?;

    Ok(SpiTransact {
        write_bytes,
        read_len,
    })
}

fn spi_transact(device: &mut Ch34xDevice, request: &SpiTransact) -> Result<Vec<u8>, String> {
    let mut read_buf = vec![0u8; request.read_len];

    device.cs_low().map_err(|e| format!("cs_low failed: {e}"))?;

    let result = device
        .spi_tx(&request.write_bytes)
        .map_err(|e| format!("spi_tx failed: {e}"))
        .and_then(|_| {
            device
                .spi_rx(&mut read_buf)
                .map_err(|e| format!("spi_rx failed: {e}"))
        });
    let cs_result = device.cs_high().map_err(|e| format!("cs_high failed: {e}"));

    match (result, cs_result) {
        (Err(e), _) => Err(e),
        (Ok(()), Err(e)) => Err(e),
        (Ok(()), Ok(())) => Ok(read_buf),
    }
}

fn handle_spi_transact(id: u64, op: &Value, device: &mut Ch34xDevice) -> Value {
    let request = match decode_spi_transact(op) {
        Ok(request) => request,
        Err((code, message)) => return error_response(id, code, message),
    };

    match spi_transact(device, &request) {
        Ok(data) => {
            eprintln!(
                "[uni_ch34x_sidecar] spi_transact write_len={} read_len={}",
                request.write_bytes.len(),
                request.read_len
            );
            success_response(id, json!({ "data_b64": STANDARD.encode(&data) }))
        }
        Err(e) => error_response(id, SERVER_ERROR, format!("spi_transact failed: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Main loop.
// ---------------------------------------------------------------------------

struct Session {
    id: String,
    device: Ch34xDevice,
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut session: Option<Session> = None;
    let mut next_session_id: u64 = 1;

    eprintln!("[uni_ch34x_sidecar] ready");

    loop {
        let payload = match read_frame(&mut reader) {
            Ok(payload) => payload,
            Err(e) => {
                eprintln!("[uni_ch34x_sidecar] read error: {e}");
                break;
            }
        };

        let request: Value = match serde_json::from_slice(&payload) {
            Ok(request) => request,
            Err(e) => {
                eprintln!("[uni_ch34x_sidecar] invalid JSON request: {e}");
                continue;
            }
        };

        let Some(id) = request.get("id").and_then(Value::as_u64) else {
            eprintln!("[uni_ch34x_sidecar] request without numeric id ignored");
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        let response = match method {
            "handshake" => {
                eprintln!("[uni_ch34x_sidecar] handshake");
                success_response(
                    id,
                    json!({
                        "protocol_version": 1,
                        "name": "uni.hal.ch34x-sidecar",
                        "version": "1.0.0",
                        "plugin_api": 1,
                        "capabilities": {
                            "spi": {
                                "pins": ["CS0", "SCK", "MOSI", "MISO"],
                                "max_frame": 4092,
                                "max_freq_khz": 60_000
                            }
                        }
                    }),
                )
            }
            "probe" => {
                eprintln!("[uni_ch34x_sidecar] probe");
                let devices = probe_devices();
                eprintln!(
                    "[uni_ch34x_sidecar] probe found {} CH34X device(s)",
                    devices.len()
                );
                success_response(id, json!({ "devices": devices }))
            }
            "open" => handle_open(id, &params, &mut session, &mut next_session_id),
            "close" => {
                let sid = params
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match session.as_ref() {
                    Some(current) if current.id == sid => {
                        eprintln!("[uni_ch34x_sidecar] close session={sid}");
                        session = None;
                        success_response(id, json!({}))
                    }
                    _ => error_response(id, BUSY, "close before open or unknown session_id"),
                }
            }
            "execute" => {
                let sid = params
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match session.as_mut() {
                    Some(current) if current.id == sid => {
                        let op = params.get("op").cloned().unwrap_or_else(|| json!({}));
                        let op_name = op.get("op").and_then(Value::as_str).unwrap_or("");
                        match op_name {
                            "spi_transact" => handle_spi_transact(id, &op, &mut current.device),
                            "gpio_set" => {
                                eprintln!("[uni_ch34x_sidecar] gpio_set denied");
                                error_response(
                                    id,
                                    CAPABILITY_NOT_EXPOSED,
                                    "capability not exposed: gpio_set",
                                )
                            }
                            other => error_response(
                                id,
                                CAPABILITY_NOT_EXPOSED,
                                format!("capability not exposed: {other}"),
                            ),
                        }
                    }
                    _ => error_response(id, BUSY, "execute before open or unknown session_id"),
                }
            }
            _ => error_response(id, METHOD_NOT_FOUND, format!("method not found: {method}")),
        };

        let response_payload = serde_json::to_vec(&response).expect("response must encode");
        if let Err(e) = write_frame(&mut writer, &response_payload) {
            eprintln!("[uni_ch34x_sidecar] write error: {e}");
            break;
        }
        if let Err(e) = writer.flush() {
            eprintln!("[uni_ch34x_sidecar] flush error: {e}");
            break;
        }
    }

    eprintln!("[uni_ch34x_sidecar] exiting");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_encode_decode_roundtrip() {
        let payload = b"{\"id\":7,\"method\":\"handshake\",\"params\":{}}";
        let frame = encode_frame(payload).expect("payload should encode");
        let mut cursor = &frame[..];
        let decoded = read_frame(&mut cursor).expect("frame should decode");
        assert_eq!(decoded, payload);
        assert!(cursor.is_empty());
    }

    #[test]
    fn spi_transact_decodes_valid_base64_and_read_len() {
        let op = json!({
            "op": "spi_transact",
            "write_b64": STANDARD.encode([0x03, 0x9F, 0x00]),
            "read_len": 4
        });
        let decoded = decode_spi_transact(&op).expect("op should decode");
        assert_eq!(decoded.write_bytes, [0x03, 0x9F, 0x00]);
        assert_eq!(decoded.read_len, 4);
    }

    #[test]
    fn spi_transact_rejects_malformed_base64() {
        let op = json!({
            "op": "spi_transact",
            "write_b64": "!!!not-base64!!!",
            "read_len": 1
        });
        let (code, message) = decode_spi_transact(&op).expect_err("base64 should be rejected");
        assert_eq!(code, INVALID_PARAMS);
        assert!(message.contains("invalid write_b64"), "{message}");
    }

    #[test]
    fn spi_transact_rejects_missing_read_len() {
        let op = json!({
            "op": "spi_transact",
            "write_b64": ""
        });
        let (code, message) =
            decode_spi_transact(&op).expect_err("missing read_len should be rejected");
        assert_eq!(code, INVALID_PARAMS);
        assert!(message.contains("missing read_len"), "{message}");
    }

    #[test]
    fn spi_transact_rejects_non_unsigned_read_len() {
        let op = json!({
            "op": "spi_transact",
            "write_b64": "",
            "read_len": -1
        });
        let (code, message) =
            decode_spi_transact(&op).expect_err("negative read_len should be rejected");
        assert_eq!(code, INVALID_PARAMS);
        assert!(message.contains("unsigned integer"), "{message}");
    }

    #[test]
    fn parses_index_and_bus_addr_device_ids() {
        let parsed = parse_ch34x_device_id("ch34x:ch341:2").expect("index id should parse");
        assert_eq!(parsed.kind, ChipKind::Ch341A);
        assert_eq!(parsed.location, DeviceLocation::DllIndex(2));

        let parsed = parse_ch34x_device_id("ch34x:ch347:3:9").expect("bus:addr id should parse");
        assert_eq!(parsed.kind, ChipKind::Ch347T);
        assert_eq!(
            parsed.location,
            DeviceLocation::UsbBusAddr { bus: 3, address: 9 }
        );

        let parsed = parse_ch34x_device_id("ch34x:ch347f:1").expect("ch347f id should parse");
        assert_eq!(parsed.kind, ChipKind::Ch347F);
        assert_eq!(parsed.location, DeviceLocation::DllIndex(1));
    }

    #[test]
    fn rejects_malformed_device_ids() {
        assert!(parse_ch34x_device_id("serprog:COM3").is_err());
        assert!(parse_ch34x_device_id("ch34x:ch999:0").is_err());
        assert!(parse_ch34x_device_id("ch34x:ch341:1:2:3").is_err());
        assert!(parse_ch34x_device_id("ch34x:ch341").is_err());
        assert!(parse_ch34x_device_id("ch34x:ch341:256:1").is_err());
    }
}
