//! `upt.chipdb` sidecar process.
//!
//! Speaks the same 4-byte little-endian length + UTF-8 JSON protocol as the
//! other `upt_hal` sidecars. Debug logs go to stderr only.

use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use upt_chipdb::Chiplib;

const FRAME_HEADER_LEN: usize = 4;
const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

const INVALID_PARAMS: i32 = -32602;
const METHOD_NOT_FOUND: i32 = -32601;
const SIDECAR_ERROR: i32 = -32000;

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

fn lib_state() -> &'static Mutex<Option<Chiplib>> {
    static LIB: OnceLock<Mutex<Option<Chiplib>>> = OnceLock::new();
    LIB.get_or_init(|| Mutex::new(None))
}

fn default_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn path_arg(params: &Value, key: &str, default: &Path) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default.to_string_lossy().into_owned())
}

fn handle_load(id: u64, params: &Value) -> Value {
    let exe_dir = default_exe_dir();
    let bin_default = exe_dir.join("chiplib.bin");
    let xml_default = exe_dir.join("chiplib.xml");
    let bin_path = path_arg(params, "bin_path", &bin_default);
    let xml_path = path_arg(params, "xml_path", &xml_default);

    match Chiplib::load_auto(&xml_path, &bin_path) {
        Ok(lib) => {
            let count = lib.entry_count();
            eprintln!("[upt.chipdb] loaded {} entries from {}", count, bin_path);
            match lib_state().lock() {
                Ok(mut guard) => {
                    *guard = Some(lib);
                    success_response(id, json!({ "count": count }))
                }
                Err(_) => error_response(id, SIDECAR_ERROR, "chipdb state lock poisoned"),
            }
        }
        Err(e) => {
            eprintln!("[upt.chipdb] load failed: {e}");
            error_response(id, SIDECAR_ERROR, e)
        }
    }
}

fn with_lib(id: u64, f: impl FnOnce(&Chiplib) -> Value) -> Value {
    match lib_state().lock() {
        Ok(guard) => match guard.as_ref() {
            Some(lib) => success_response(id, f(lib)),
            None => error_response(id, SIDECAR_ERROR, "NOT_LOADED"),
        },
        Err(_) => error_response(id, SIDECAR_ERROR, "chipdb state lock poisoned"),
    }
}

fn handle_lookup(id: u64, params: &Value) -> Value {
    let Some(chip_id) = params.get("id").and_then(Value::as_str) else {
        return error_response(id, INVALID_PARAMS, "missing id");
    };

    match lib_state().lock() {
        Ok(guard) => match guard.as_ref() {
            Some(lib) => match lib.find_by_id(chip_id) {
                Some(info) => success_response(
                    id,
                    json!({
                        "id": info.id,
                        "vendor": info.vendor,
                        "model": info.model,
                        "protocol": info.protocol,
                        "size": info.size,
                        "page": info.page,
                        "attrs": info.attrs,
                    }),
                ),
                None => error_response(id, SIDECAR_ERROR, "NOT_FOUND"),
            },
            None => error_response(id, SIDECAR_ERROR, "NOT_LOADED"),
        },
        Err(_) => error_response(id, SIDECAR_ERROR, "chipdb state lock poisoned"),
    }
}

fn handle_vendors(id: u64, params: &Value) -> Value {
    let Some(protocol) = params.get("protocol").and_then(Value::as_str) else {
        return error_response(id, INVALID_PARAMS, "missing protocol");
    };
    with_lib(id, |lib| json!({ "vendors": lib.list_vendors(protocol) }))
}

fn handle_models(id: u64, params: &Value) -> Value {
    let Some(protocol) = params.get("protocol").and_then(Value::as_str) else {
        return error_response(id, INVALID_PARAMS, "missing protocol");
    };
    let Some(vendor) = params.get("vendor").and_then(Value::as_str) else {
        return error_response(id, INVALID_PARAMS, "missing vendor");
    };
    with_lib(
        id,
        |lib| json!({ "models": lib.list_models(protocol, vendor) }),
    )
}

fn handle_stats(id: u64) -> Value {
    with_lib(id, |lib| json!({ "count": lib.entry_count() }))
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    eprintln!("[upt.chipdb] ready");

    loop {
        let payload = match read_frame(&mut reader) {
            Ok(payload) => payload,
            Err(e) => {
                eprintln!("[upt.chipdb] read error: {e}");
                break;
            }
        };

        let request: Value = match serde_json::from_slice(&payload) {
            Ok(request) => request,
            Err(e) => {
                eprintln!("[upt.chipdb] invalid JSON request: {e}");
                continue;
            }
        };

        let Some(id) = request.get("id").and_then(Value::as_u64) else {
            eprintln!("[upt.chipdb] request without numeric id ignored");
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        eprintln!("[upt.chipdb] method={method}");

        let response = match method {
            "handshake" => {
                eprintln!(
                    "[upt.chipdb] handshake name={} version={}",
                    params.get("name").and_then(Value::as_str).unwrap_or("?"),
                    params.get("version").and_then(Value::as_str).unwrap_or("?")
                );
                success_response(
                    id,
                    json!({
                        "name": "upt.chipdb",
                        "version": "1.0.0",
                        "plugin_api": 1,
                    }),
                )
            }
            "load" => handle_load(id, &params),
            "lookup" => handle_lookup(id, &params),
            "vendors" => handle_vendors(id, &params),
            "models" => handle_models(id, &params),
            "stats" => handle_stats(id),
            _ => error_response(id, METHOD_NOT_FOUND, format!("method not found: {method}")),
        };

        let response_payload = serde_json::to_vec(&response).expect("response must encode");
        if let Err(e) = write_frame(&mut writer, &response_payload) {
            eprintln!("[upt.chipdb] write error: {e}");
            break;
        }
        if let Err(e) = writer.flush() {
            eprintln!("[upt.chipdb] flush error: {e}");
            break;
        }
    }

    eprintln!("[upt.chipdb] exiting");
}
