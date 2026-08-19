//! Serprog sidecar adapter process for the v1 sidecar protocol.
//!
//! It speaks framed JSON-RPC on stdin/stdout. Debug logs go to stderr only;
//! stdout is reserved for protocol frames.
//!
//! `probe` lists available serial ports without touching them; `open` and
//! `execute` delegate to the in-process [`serprog`] adapter.

#[path = "../src/serprog.rs"]
#[allow(dead_code)] // in-process adapter has optional helpers the sidecar does not call
mod serprog;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

const FRAME_HEADER_LEN: usize = 4;
const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

const CAPABILITY_NOT_EXPOSED: i32 = -32001;
const BUSY: i32 = -32002;
const INVALID_PARAMS: i32 = -32602;
const METHOD_NOT_FOUND: i32 = -32601;
const SERVER_ERROR: i32 = -32000;

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

fn success_response(id: u64, result: Value) -> Value {
    json!({ "id": id, "result": result })
}

fn error_response(id: u64, code: i32, message: impl Into<String>) -> Value {
    json!({ "id": id, "error": { "code": code, "message": message.into() } })
}

fn handle_spi_transact(id: u64, op: &Value, device: &mut serprog::Serprog) -> Value {
    let Some(write_b64) = op.get("write_b64").and_then(Value::as_str) else {
        return error_response(id, INVALID_PARAMS, "missing write_b64");
    };
    let Some(read_len) = op.get("read_len").and_then(Value::as_u64) else {
        return error_response(id, INVALID_PARAMS, "missing read_len");
    };
    let Ok(read_len) = usize::try_from(read_len) else {
        return error_response(id, INVALID_PARAMS, "read_len does not fit usize");
    };

    let write_bytes = match STANDARD.decode(write_b64) {
        Ok(bytes) => bytes,
        Err(e) => return error_response(id, INVALID_PARAMS, format!("invalid write_b64: {e}")),
    };

    match device.spi_command(&write_bytes, read_len) {
        Ok(data) => {
            eprintln!(
                "[serprog_sidecar] spi_transact write_len={} read_len={read_len}",
                write_bytes.len()
            );
            success_response(id, json!({ "data_b64": STANDARD.encode(&data) }))
        }
        Err(e) => error_response(id, SERVER_ERROR, format!("spi_transact failed: {e}")),
    }
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut session: Option<(String, serprog::Serprog)> = None;
    let mut next_session_id: u64 = 1;

    eprintln!("[serprog_sidecar] ready");

    loop {
        let payload = match read_frame(&mut reader) {
            Ok(payload) => payload,
            Err(e) => {
                eprintln!("[serprog_sidecar] read error: {e}");
                break;
            }
        };

        let request: Value = match serde_json::from_slice(&payload) {
            Ok(request) => request,
            Err(e) => {
                eprintln!("[serprog_sidecar] invalid JSON request: {e}");
                continue;
            }
        };

        let Some(id) = request.get("id").and_then(Value::as_u64) else {
            eprintln!("[serprog_sidecar] request without numeric id ignored");
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        let response = match method {
            "handshake" => {
                eprintln!(
                    "[serprog_sidecar] handshake name={} version={}",
                    params.get("name").and_then(Value::as_str).unwrap_or("?"),
                    params.get("version").and_then(Value::as_str).unwrap_or("?")
                );
                success_response(id, json!({ "protocol_version": 1 }))
            }
            "probe" => {
                eprintln!("[serprog_sidecar] probe");
                match serialport::available_ports() {
                    Ok(ports) => {
                        let devices = ports
                            .into_iter()
                            .filter(|info| !info.port_name.trim().is_empty())
                            .map(|info| {
                                json!({
                                    "id": format!("serprog:{}", info.port_name),
                                    "kind": "serprog",
                                    "detail": info.port_name,
                                })
                            })
                            .collect::<Vec<_>>();
                        eprintln!(
                            "[serprog_sidecar] probe found {} serial port(s)",
                            devices.len()
                        );
                        success_response(id, json!({ "devices": devices }))
                    }
                    Err(e) => error_response(
                        id,
                        SERVER_ERROR,
                        format!("failed to enumerate serial ports: {e}"),
                    ),
                }
            }
            "open" => {
                let device_id = params
                    .get("device_id")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                eprintln!("[serprog_sidecar] open device={device_id}");
                match device_id.strip_prefix("serprog:") {
                    Some(port) if !port.is_empty() => match serprog::Serprog::open(port) {
                        Ok(device) => {
                            let sid = format!("serprog-session-{next_session_id}");
                            next_session_id += 1;
                            session = Some((sid.clone(), device));
                            success_response(id, json!({ "session_id": sid }))
                        }
                        Err(e) => error_response(
                            id,
                            SERVER_ERROR,
                            format!("failed to open serprog port {port}: {e}"),
                        ),
                    },
                    Some(_) => error_response(
                        id,
                        INVALID_PARAMS,
                        "device_id must include a port name after 'serprog:'",
                    ),
                    None => error_response(
                        id,
                        INVALID_PARAMS,
                        format!("device_id must start with 'serprog:': {device_id}"),
                    ),
                }
            }
            "close" => {
                let sid = params
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match session.as_ref() {
                    Some((current_sid, _)) if current_sid == sid => {
                        eprintln!("[serprog_sidecar] close session={sid}");
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
                    Some((current_sid, device)) if current_sid == sid => {
                        let op = params.get("op").cloned().unwrap_or_else(|| json!({}));
                        let op_name = op.get("op").and_then(Value::as_str).unwrap_or("");
                        match op_name {
                            "spi_transact" => handle_spi_transact(id, &op, device),
                            "gpio_set" => {
                                eprintln!("[serprog_sidecar] gpio_set denied");
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
            eprintln!("[serprog_sidecar] write error: {e}");
            break;
        }
        if let Err(e) = writer.flush() {
            eprintln!("[serprog_sidecar] flush error: {e}");
            break;
        }
    }

    eprintln!("[serprog_sidecar] exiting");
}
