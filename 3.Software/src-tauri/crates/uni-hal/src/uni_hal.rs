//! Transport-agnostic HAL client for the sidecar adapter protocol.
//!
//! This module is intentionally Tauri-free. It defines the v1 sidecar
//! protocol client ([`SidecarClient`]), the message-level transport trait
//! ([`SidecarTransport`]) and a small framing codec ([`frame`]) for the
//! wire format used by sidecar processes on stdio:
//!
//! ```text
//! +----------------+----------------------+
//! | 4 bytes LE len | UTF-8 JSON payload   |
//! +----------------+----------------------+
//! ```

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Current sidecar protocol version.
pub const SIDECAR_PROTOCOL_VERSION: u32 = 1;

/// Error code returned by sidecars when a capability was not declared.
const ERR_CAPABILITY_NOT_EXPOSED: i32 = -32001;
/// Error code returned by sidecars when an adapter/session is busy.
const ERR_BUSY: i32 = -32002;

/// A message-level transport for sidecar protocol frames.
///
/// Implementations are responsible for their own wire framing (for example
/// the 4-byte little-endian length prefix used over stdio). `send` delivers
/// one complete payload and `recv` returns the next complete payload.
pub trait SidecarTransport: Send {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String>;
    fn recv(&mut self) -> Result<Vec<u8>, String>;
}

/// A device reported by a sidecar `probe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarDevice {
    pub id: String,
    pub kind: String,
    pub detail: String,
}

/// Transport-agnostic JSON-RPC 2.0 style client for one sidecar adapter.
pub struct SidecarClient {
    transport: Box<dyn SidecarTransport>,
    next_id: u64,
}

impl SidecarClient {
    pub fn new(transport: Box<dyn SidecarTransport>) -> Self {
        Self {
            transport,
            next_id: 1,
        }
    }

    /// Perform the `handshake` method and verify the protocol version.
    pub fn handshake(
        &mut self,
        name: &str,
        version: &str,
        capabilities: &uni_plugin::CapabilitySet,
    ) -> Result<(), String> {
        let params = json!({
            "name": name,
            "version": version,
            "plugin_api": uni_plugin::PLUGIN_API_VERSION,
            "capabilities": capabilities,
        });
        let result = self.request("handshake", params)?;
        let protocol_version = result
            .get("protocol_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "handshake response missing protocol_version".to_string())?;
        if protocol_version != u64::from(SIDECAR_PROTOCOL_VERSION) {
            return Err(format!(
                "unsupported sidecar protocol version {protocol_version} (expected {SIDECAR_PROTOCOL_VERSION})"
            ));
        }
        Ok(())
    }

    /// Ask the sidecar to report its candidate devices.
    pub fn probe(&mut self) -> Result<Vec<SidecarDevice>, String> {
        #[derive(Deserialize)]
        struct ProbeResult {
            devices: Vec<SidecarDevice>,
        }

        let result = self.request("probe", json!({}))?;
        let probe_result: ProbeResult =
            serde_json::from_value(result).map_err(|e| format!("invalid probe response: {e}"))?;
        Ok(probe_result.devices)
    }

    /// Open a session for `device_id` and return the session id.
    pub fn open(&mut self, device_id: &str) -> Result<String, String> {
        let result = self.request("open", json!({ "device_id": device_id }))?;
        result
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "open response missing session_id".to_string())
    }

    /// Close a session.
    pub fn close(&mut self, session_id: &str) -> Result<(), String> {
        let _ = self.request("close", json!({ "session_id": session_id }))?;
        Ok(())
    }

    /// SPI full-duplex transaction: clock `write_bytes` out while capturing
    /// `read_len` bytes from the adapter.
    pub fn spi_transact(
        &mut self,
        session_id: &str,
        write_bytes: &[u8],
        read_len: usize,
    ) -> Result<Vec<u8>, String> {
        let op = json!({
            "op": "spi_transact",
            "write_b64": STANDARD.encode(write_bytes),
            "read_len": read_len,
        });
        let result = self.execute(session_id, &op)?;
        let data_b64 = result
            .get("data_b64")
            .and_then(Value::as_str)
            .ok_or_else(|| "spi_transact response missing data_b64".to_string())?;
        STANDARD
            .decode(data_b64)
            .map_err(|e| format!("invalid base64 in spi_transact response: {e}"))
    }

    /// Generic `execute` call. `op` must be a JSON object such as
    /// `{"op":"spi_transact","write_b64":"...","read_len":4}`.
    pub fn execute(&mut self, session_id: &str, op: &Value) -> Result<Value, String> {
        if !op.is_object() {
            return Err("execute op must be a JSON object".to_string());
        }
        self.request("execute", json!({ "session_id": session_id, "op": op }))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({ "id": id, "method": method, "params": params });
        let payload = serde_json::to_vec(&request)
            .map_err(|e| format!("failed to encode '{method}' request: {e}"))?;
        self.transport
            .send(&payload)
            .map_err(|e| format!("failed to send '{method}' request: {e}"))?;

        let response_payload = self
            .transport
            .recv()
            .map_err(|e| format!("failed to receive '{method}' response: {e}"))?;
        let response: Value = serde_json::from_slice(&response_payload)
            .map_err(|e| format!("invalid JSON in '{method}' response: {e}"))?;

        let response_id = response
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("'{method}' response missing numeric id"))?;
        if response_id != id {
            return Err(format!(
                "'{method}' response id mismatch: expected {id}, got {response_id}"
            ));
        }

        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(describe_rpc_error(code, message));
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("'{method}' response missing result"))
    }
}

/// A real child-process transport that speaks framed JSON on the child's
/// stdin/stdout. Stderr is inherited so sidecar debug logs stay visible.
pub struct ChildTransport {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
}

impl ChildTransport {
    /// Spawn `program` with `args` as a sidecar process.
    ///
    /// The child is started with piped stdin/stdout and inherited stderr.
    /// No shell is involved.
    pub fn spawn_child(program: &str, args: &[String]) -> Result<Self, String> {
        let mut command = Command::new(program);
        command.args(args);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::inherit());

        let mut child = command
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar process '{program}': {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            "failed to capture sidecar stdout pipe".to_string()
        })?;

        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }
}

impl SidecarTransport for ChildTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "sidecar stdin is not connected".to_string())?;
        frame::write_frame(stdin, bytes)
    }

    fn recv(&mut self) -> Result<Vec<u8>, String> {
        frame::read_frame(&mut self.stdout)
    }
}

impl Drop for ChildTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn a sidecar adapter process and perform the protocol handshake.
pub fn spawn_sidecar(
    program: &str,
    args: &[String],
    name: &str,
    version: &str,
    capabilities: &uni_plugin::CapabilitySet,
) -> Result<SidecarClient, String> {
    let transport = ChildTransport::spawn_child(program, args)?;
    let mut client = SidecarClient::new(Box::new(transport));
    client.handshake(name, version, capabilities)?;
    Ok(client)
}

fn describe_rpc_error(code: i64, message: &str) -> String {
    if code == i64::from(ERR_CAPABILITY_NOT_EXPOSED) {
        format!("CAPABILITY_NOT_EXPOSED: {message}")
    } else if code == i64::from(ERR_BUSY) {
        format!("BUSY: {message}")
    } else {
        format!("sidecar error {code}: {message}")
    }
}

/// Wire framing codec shared by sidecar transports.
pub mod frame {
    use std::io::{Read, Write};

    pub const HEADER_LEN: usize = 4;
    /// v1 frames are for validation traffic only, so keep a hard ceiling to
    /// avoid unbounded allocation from a corrupt length field.
    pub const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

    /// Encode `payload` as a 4-byte little-endian length prefix plus payload.
    pub fn encode(payload: &[u8]) -> Result<Vec<u8>, String> {
        let len = u32::try_from(payload.len())
            .map_err(|_| format!("payload too large to frame: {} bytes", payload.len()))?;
        let mut frame = Vec::with_capacity(HEADER_LEN + len as usize);
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(payload);
        Ok(frame)
    }

    /// Write one complete frame to `writer`.
    pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), String> {
        let frame = encode(payload)?;
        writer
            .write_all(&frame)
            .map_err(|e| format!("failed to write frame: {e}"))
    }

    /// Read one complete frame from `reader`.
    pub fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
        let mut header = [0u8; HEADER_LEN];
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
}

/// Test-only in-memory transports and a tiny mock sidecar server loop.
#[cfg(test)]
pub mod testing {
    use super::SidecarTransport;
    use std::sync::mpsc::{channel, Receiver, Sender};

    /// One end of a paired in-memory message channel.
    pub struct ChannelTransport {
        tx: Sender<Vec<u8>>,
        rx: Receiver<Vec<u8>>,
    }

    impl ChannelTransport {
        fn new(tx: Sender<Vec<u8>>, rx: Receiver<Vec<u8>>) -> Self {
            Self { tx, rx }
        }
    }

    impl SidecarTransport for ChannelTransport {
        fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.tx
                .send(bytes.to_vec())
                .map_err(|_| "mock channel send failed".to_string())
        }

        fn recv(&mut self) -> Result<Vec<u8>, String> {
            self.rx
                .recv()
                .map_err(|_| "mock channel recv failed".to_string())
        }
    }

    /// A pair of connected in-memory transports. `client` and `server` are
    /// opposite ends of two mpsc channels and can be used to run a mock
    /// sidecar server loop in a test thread.
    pub struct PairedTransport {
        pub client: ChannelTransport,
        pub server: ChannelTransport,
    }

    impl PairedTransport {
        pub fn new() -> Self {
            let (client_tx, server_rx) = channel();
            let (server_tx, client_rx) = channel();
            Self {
                client: ChannelTransport::new(client_tx, client_rx),
                server: ChannelTransport::new(server_tx, server_rx),
            }
        }
    }

    impl Default for PairedTransport {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::PairedTransport;
    use super::*;
    use std::thread;
    use uni_plugin::{CapabilitySet, SpiCapability};

    const JEDEC_ID: [u8; 3] = [0xEF, 0x40, 0x18];

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

    fn spawn_mock_server(mut server: super::testing::ChannelTransport) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            while let Ok(payload) = server.recv() {
                let Ok(request) = serde_json::from_slice::<Value>(&payload) else {
                    continue;
                };
                let Some(id) = request.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                let method = request.get("method").and_then(Value::as_str).unwrap_or("");
                let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

                let response = match method {
                    "handshake" => json!({ "id": id, "result": { "protocol_version": 1 } }),
                    "probe" => json!({
                        "id": id,
                        "result": {
                            "devices": [
                                { "id": "mock-0", "kind": "spi", "detail": "Mock SPI adapter" }
                            ]
                        }
                    }),
                    "open" => json!({ "id": id, "result": { "session_id": "sess-1" } }),
                    "close" => json!({ "id": id, "result": {} }),
                    "execute" => {
                        let session_id = params.get("session_id").and_then(Value::as_str);
                        if session_id != Some("sess-1") {
                            json!({
                                "id": id,
                                "error": { "code": -32002, "message": "execute before open or unknown session_id" }
                            })
                        } else {
                            let op = params.get("op").cloned().unwrap_or_else(|| json!({}));
                            let op_name = op.get("op").and_then(Value::as_str).unwrap_or("");
                            match op_name {
                                "spi_transact" => {
                                    let write_b64 =
                                        op.get("write_b64").and_then(Value::as_str).unwrap_or("");
                                    let read_len =
                                        op.get("read_len").and_then(Value::as_u64).unwrap_or(0)
                                            as usize;
                                    let write_bytes =
                                        STANDARD.decode(write_b64).unwrap_or_default();
                                    let mut data = vec![0xFFu8; read_len];
                                    if write_bytes.contains(&0x9F) {
                                        for (i, byte) in JEDEC_ID.iter().enumerate().take(read_len)
                                        {
                                            data[i] = *byte;
                                        }
                                    }
                                    json!({
                                        "id": id,
                                        "result": { "data_b64": STANDARD.encode(&data) }
                                    })
                                }
                                _ => json!({
                                    "id": id,
                                    "error": { "code": -32001, "message": format!("capability not exposed: {op_name}") }
                                }),
                            }
                        }
                    }
                    _ => json!({
                        "id": id,
                        "error": { "code": -32601, "message": format!("method not found: {method}") }
                    }),
                };

                let Ok(response_payload) = serde_json::to_vec(&response) else {
                    continue;
                };
                if server.send(&response_payload).is_err() {
                    break;
                }
            }
        })
    }

    fn open_mock_session(client: &mut SidecarClient) -> String {
        client
            .handshake("uni-hal-test", "0.1.0", &spi_capabilities())
            .expect("handshake should succeed");
        let devices = client.probe().expect("probe should succeed");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "mock-0");
        client.open("mock-0").expect("open should succeed")
    }

    #[test]
    fn handshake_probe_open_spi_roundtrip() {
        let pair = PairedTransport::new();
        let server = spawn_mock_server(pair.server);
        let mut client = SidecarClient::new(Box::new(pair.client));

        let session = open_mock_session(&mut client);
        assert_eq!(session, "sess-1");

        let jedec = client
            .spi_transact(&session, &[0x9F], 4)
            .expect("spi_transact should succeed");
        assert_eq!(jedec, vec![0xEF, 0x40, 0x18, 0xFF]);

        let padded = client
            .spi_transact(&session, &[0x00], 2)
            .expect("spi_transact should succeed");
        assert_eq!(padded, vec![0xFF, 0xFF]);

        client.close(&session).expect("close should succeed");
        drop(client);
        server.join().expect("mock server should exit");
    }

    #[test]
    fn undeclared_capability_returns_capability_not_exposed() {
        let pair = PairedTransport::new();
        let server = spawn_mock_server(pair.server);
        let mut client = SidecarClient::new(Box::new(pair.client));

        let session = open_mock_session(&mut client);

        let err = client
            .execute(
                &session,
                &json!({ "op": "gpio_set", "pin": "IO0", "level": true }),
            )
            .expect_err("gpio_set must be rejected");
        assert!(err.contains("CAPABILITY_NOT_EXPOSED"), "{err}");

        let err = client
            .execute(&session, &json!({ "op": "i2c_transfer" }))
            .expect_err("unknown op must be rejected");
        assert!(err.contains("CAPABILITY_NOT_EXPOSED"), "{err}");

        drop(client);
        server.join().expect("mock server should exit");
    }

    #[test]
    fn truncated_frame_length_is_readable_error() {
        let mut truncated_header: &[u8] = &[0x01, 0x00];
        let err = frame::read_frame(&mut truncated_header).expect_err("truncated header");
        assert!(err.contains("frame length header"), "{err}");

        let mut truncated_payload: &[u8] = &[0x04, 0x00, 0x00, 0x00, 0x7B];
        let err = frame::read_frame(&mut truncated_payload).expect_err("truncated payload");
        assert!(err.contains("frame payload"), "{err}");
    }
}
