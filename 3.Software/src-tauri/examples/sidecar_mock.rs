//! Mock sidecar adapter process for the v1 sidecar protocol.
//!
//! It speaks framed JSON-RPC on stdin/stdout. Debug logs go to stderr only;
//! stdout is reserved for protocol frames.
//!
//! The mock models a synchronous 1 MiB SPI NOR flash (W25Q128-style JEDEC ID
//! but truncated to 1 MiB). Each open session receives a fresh erased model.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

const FRAME_HEADER_LEN: usize = 4;
const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

const CAPABILITY_NOT_EXPOSED: i32 = -32001;
const BUSY: i32 = -32002;
const INVALID_PARAMS: i32 = -32602;
const METHOD_NOT_FOUND: i32 = -32601;

const FLASH_SIZE: usize = 1024 * 1024;
const SECTOR_SIZE: usize = 4 * 1024;
const BLOCK_SIZE: usize = 64 * 1024;

/// One open session: a private 1 MiB NOR model plus its write-enable latch.
struct FlashSession {
    id: String,
    flash: Vec<u8>,
    wel: bool,
}

impl FlashSession {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            flash: vec![0xFF; FLASH_SIZE],
            wel: false,
        }
    }
}

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

fn require_wel(wel: bool, command: &str) -> Result<(), (i32, String)> {
    if !wel {
        return Err((
            BUSY,
            format!("{command} requires write enable (send 0x06 first)"),
        ));
    }
    Ok(())
}

fn parse_addr(write: &[u8], command: &str) -> Result<usize, (i32, String)> {
    if write.len() < 4 {
        return Err((
            INVALID_PARAMS,
            format!("{command} expects 3 address bytes after the opcode"),
        ));
    }
    Ok(((write[1] as usize) << 16) | ((write[2] as usize) << 8) | write[3] as usize)
}

/// Emulate one synchronous SPI NOR transaction against the session flash
/// model. Returns exactly `read_len` bytes.
fn spi_transact(
    flash: &mut [u8],
    wel: &mut bool,
    write: &[u8],
    read_len: usize,
) -> Result<Vec<u8>, (i32, String)> {
    let mut data = vec![0xFFu8; read_len];

    if write.is_empty() {
        return Ok(data);
    }

    match write[0] {
        // JEDEC ID: report W25Q128 values with 0xFF padding.
        0x9F => {
            let jedec = [0xEF, 0x40, 0x18];
            for (i, byte) in jedec.iter().enumerate().take(read_len) {
                data[i] = *byte;
            }
        }
        // READ DATA: 24-bit address, reads past the model return 0xFF.
        0x03 => {
            let addr = parse_addr(write, "READ_DATA (0x03)")?;
            for (i, byte) in data.iter_mut().enumerate() {
                let index = addr.saturating_add(i);
                if index < flash.len() {
                    *byte = flash[index];
                }
            }
        }
        // WRITE ENABLE: set the in-memory latch.
        0x06 => {
            *wel = true;
        }
        // PAGE PROGRAM: AND-into-flash semantics, clears WEL.
        0x02 => {
            let addr = parse_addr(write, "PAGE_PROGRAM (0x02)")?;
            require_wel(*wel, "PAGE_PROGRAM (0x02)")?;
            for (i, byte) in write.iter().skip(4).enumerate() {
                let index = addr.saturating_add(i);
                if index < flash.len() {
                    flash[index] &= *byte;
                }
            }
            *wel = false;
        }
        // SECTOR ERASE (4 KiB): aligned inside the model.
        0x20 => {
            let addr = parse_addr(write, "SECTOR_ERASE (0x20)")?;
            require_wel(*wel, "SECTOR_ERASE (0x20)")?;
            let start = addr & !(SECTOR_SIZE - 1);
            if start < flash.len() {
                let end = (start + SECTOR_SIZE).min(flash.len());
                flash[start..end].fill(0xFF);
            }
            *wel = false;
        }
        // BLOCK ERASE (64 KiB): aligned inside the model.
        0xD8 => {
            let addr = parse_addr(write, "BLOCK_ERASE (0xD8)")?;
            require_wel(*wel, "BLOCK_ERASE (0xD8)")?;
            let start = addr & !(BLOCK_SIZE - 1);
            if start < flash.len() {
                let end = (start + BLOCK_SIZE).min(flash.len());
                flash[start..end].fill(0xFF);
            }
            *wel = false;
        }
        // CHIP ERASE: erase the whole 1 MiB model.
        0xC7 => {
            require_wel(*wel, "CHIP_ERASE (0xC7)")?;
            flash.fill(0xFF);
            *wel = false;
        }
        // READ STATUS: the model is always ready, so report 0x00.
        0x05 => {
            if let Some(first) = data.first_mut() {
                *first = 0x00;
            }
        }
        _ => {
            eprintln!(
                "[sidecar_mock] spi_transact unknown opcode 0x{:02X}, returning 0xFF padding",
                write[0]
            );
        }
    }

    Ok(data)
}

fn handle_spi_transact(id: u64, op: &Value, session: &mut FlashSession) -> Value {
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

    let data = match spi_transact(&mut session.flash, &mut session.wel, &write_bytes, read_len) {
        Ok(data) => data,
        Err((code, message)) => return error_response(id, code, message),
    };

    eprintln!(
        "[sidecar_mock] spi_transact write_len={} read_len={read_len}",
        write_bytes.len()
    );
    success_response(id, json!({ "data_b64": STANDARD.encode(&data) }))
}

fn handle_execute(id: u64, params: &Value, session: Option<&mut FlashSession>) -> Value {
    let Some(sid) = params.get("session_id").and_then(Value::as_str) else {
        return error_response(id, BUSY, "execute before open or missing session_id");
    };
    let Some(session) = session else {
        return error_response(id, BUSY, "execute before open or unknown session_id");
    };
    if session.id != sid {
        return error_response(id, BUSY, "execute before open or unknown session_id");
    }

    let op = params.get("op").cloned().unwrap_or_else(|| json!({}));
    let op_name = op.get("op").and_then(Value::as_str).unwrap_or("");

    match op_name {
        "spi_transact" => handle_spi_transact(id, &op, session),
        _ => error_response(
            id,
            CAPABILITY_NOT_EXPOSED,
            format!("capability not exposed: {op_name}"),
        ),
    }
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut session: Option<FlashSession> = None;

    eprintln!("[sidecar_mock] ready");

    loop {
        let payload = match read_frame(&mut reader) {
            Ok(payload) => payload,
            Err(e) => {
                eprintln!("[sidecar_mock] read error: {e}");
                break;
            }
        };

        let request: Value = match serde_json::from_slice(&payload) {
            Ok(request) => request,
            Err(e) => {
                eprintln!("[sidecar_mock] invalid JSON request: {e}");
                continue;
            }
        };

        let Some(id) = request.get("id").and_then(Value::as_u64) else {
            eprintln!("[sidecar_mock] request without numeric id ignored");
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        let response = match method {
            "handshake" => {
                eprintln!(
                    "[sidecar_mock] handshake name={} version={}",
                    params.get("name").and_then(Value::as_str).unwrap_or("?"),
                    params.get("version").and_then(Value::as_str).unwrap_or("?")
                );
                success_response(id, json!({ "protocol_version": 1 }))
            }
            "probe" => {
                eprintln!("[sidecar_mock] probe");
                success_response(
                    id,
                    json!({
                        "devices": [
                            { "id": "mock-0", "kind": "spi", "detail": "Mock sidecar adapter" }
                        ]
                    }),
                )
            }
            "open" => {
                let device_id = params
                    .get("device_id")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                eprintln!("[sidecar_mock] open device={device_id}");
                let sid = "mock-session-1".to_string();
                session = Some(FlashSession::new(&sid));
                success_response(id, json!({ "session_id": sid }))
            }
            "close" => {
                let sid = params
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if session.as_ref().map(|s| s.id.as_str()) == Some(sid) {
                    eprintln!("[sidecar_mock] close session={sid}");
                    session = None;
                    success_response(id, json!({}))
                } else {
                    error_response(id, BUSY, "close before open or unknown session_id")
                }
            }
            "execute" => handle_execute(id, &params, session.as_mut()),
            _ => error_response(id, METHOD_NOT_FOUND, format!("method not found: {method}")),
        };

        let response_payload = serde_json::to_vec(&response).expect("response must encode");
        if let Err(e) = write_frame(&mut writer, &response_payload) {
            eprintln!("[sidecar_mock] write error: {e}");
            break;
        }
        if let Err(e) = writer.flush() {
            eprintln!("[sidecar_mock] flush error: {e}");
            break;
        }
    }

    eprintln!("[sidecar_mock] exiting");
}
