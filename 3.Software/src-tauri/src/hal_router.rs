//! HAL routing layer over sidecar adapter plugins.
//!
//! This module is intentionally Tauri-free. It turns the in-memory plugin
//! registry ([`crate::plugin::PluginManager`]) into live sidecar adapter
//! sessions and routes HAL operations such as SPI transactions to the
//! matching adapter.

use crate::plugin::{PluginKind, PluginManager};
use crate::uni_hal::{self, SidecarClient, SidecarDevice};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A successfully spawned and probed sidecar adapter plugin.
pub struct LoadedAdapter {
    pub name: String,
    pub path: PathBuf,
    pub client: SidecarClient,
    pub devices: Vec<SidecarDevice>,
}

/// An open session between a HAL client and an adapter device.
pub struct AdapterSession {
    pub adapter: String,
    pub device_id: String,
    pub session_id: String,
}

/// Adapter + device pair selected for a sidecar-backed chip operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarSelection {
    pub adapter: String,
    pub device_id: String,
}

/// Runtime HAL router: adapters, open sessions, and the per-adapter error
/// log.
pub struct HalRouter {
    pub adapters: Vec<LoadedAdapter>,
    pub sessions: Vec<AdapterSession>,
    pub errors: Vec<(String, String)>,
}

impl HalRouter {
    /// Spawn every enabled adapter plugin and probe its devices.
    ///
    /// Per-adapter failures (missing entry, spawn failure, handshake failure,
    /// probe failure) are recorded in [`HalRouter::errors`] and skipped; they
    /// never abort the whole startup.
    pub fn start(manager: &mut PluginManager, app_root: &Path) -> Self {
        let _ = app_root;
        let mut adapters = Vec::new();
        let mut errors = Vec::new();

        for loaded in &manager.plugins {
            if loaded.manifest.kind != PluginKind::Adapter || !loaded.enabled {
                continue;
            }

            let entry_path = loaded.path.join(&loaded.manifest.entry);
            if !entry_path.exists() {
                errors.push((
                    loaded.manifest.name.clone(),
                    format!("adapter entry does not exist: {}", entry_path.display()),
                ));
                continue;
            }
            if !entry_path.is_file() {
                errors.push((
                    loaded.manifest.name.clone(),
                    format!("adapter entry is not a file: {}", entry_path.display()),
                ));
                continue;
            }

            let program = entry_path.as_os_str().to_str().unwrap_or("");
            match uni_hal::spawn_sidecar(
                program,
                &[],
                &loaded.manifest.name,
                &loaded.manifest.version.to_string(),
                &loaded.manifest.capabilities,
            ) {
                Ok(mut client) => match client.probe() {
                    Ok(devices) => adapters.push(LoadedAdapter {
                        name: loaded.manifest.name.clone(),
                        path: entry_path,
                        client,
                        devices,
                    }),
                    Err(e) => {
                        errors.push((loaded.manifest.name.clone(), format!("probe failed: {e}")))
                    }
                },
                Err(e) => errors.push((loaded.manifest.name.clone(), e)),
            }
        }

        Self {
            adapters,
            sessions: Vec::new(),
            errors,
        }
    }

    /// Clone the current adapter -> devices map.
    pub fn adapter_devices(&self) -> Vec<(String, Vec<SidecarDevice>)> {
        self.adapters
            .iter()
            .map(|adapter| (adapter.name.clone(), adapter.devices.clone()))
            .collect()
    }

    /// Open a session on an adapter device.
    ///
    /// If a session for the same adapter and device is already open it is
    /// reused as-is. If the adapter has another active session it is closed
    /// best-effort first, then a new sidecar session is opened for the
    /// requested device.
    pub fn open(&mut self, adapter_name: &str, device_id: &str) -> Result<String, String> {
        if let Some(session) = self
            .sessions
            .iter()
            .find(|s| s.adapter == adapter_name && s.device_id == device_id)
        {
            return Ok(session.session_id.clone());
        }

        if let Some(index) = self.sessions.iter().position(|s| s.adapter == adapter_name) {
            let existing_device_id = self.sessions[index].device_id.clone();
            let _ = self.close(adapter_name, &existing_device_id);
        }

        let adapter = self
            .adapters
            .iter_mut()
            .find(|adapter| adapter.name == adapter_name)
            .ok_or_else(|| format!("unknown adapter '{adapter_name}'"))?;

        let session_id = adapter.client.open(device_id)?;
        self.sessions.push(AdapterSession {
            adapter: adapter_name.to_string(),
            device_id: device_id.to_string(),
            session_id: session_id.clone(),
        });
        Ok(session_id)
    }

    /// Close the session for `(adapter_name, device_id)`.
    ///
    /// The sidecar session is closed best-effort (errors are recorded in
    /// [`HalRouter::errors`]) and the local session is removed. A missing
    /// session is the only error returned.
    pub fn close(&mut self, adapter_name: &str, device_id: &str) -> Result<(), String> {
        let Some(index) = self
            .sessions
            .iter()
            .position(|s| s.adapter == adapter_name && s.device_id == device_id)
        else {
            return Err("BUSY: 会话不存在，请先 open".to_string());
        };

        let session = self.sessions.remove(index);
        if let Some(adapter) = self
            .adapters
            .iter_mut()
            .find(|adapter| adapter.name == adapter_name)
        {
            if let Err(e) = adapter.client.close(&session.session_id) {
                self.errors
                    .push((adapter_name.to_string(), format!("close failed: {e}")));
            }
        }
        Ok(())
    }

    /// Route an SPI full-duplex transaction to an open adapter session.
    pub fn spi_transact(
        &mut self,
        adapter_name: &str,
        device_id: &str,
        write: &[u8],
        read_len: usize,
    ) -> Result<Vec<u8>, String> {
        let session_id = self
            .sessions
            .iter()
            .find(|s| s.adapter == adapter_name && s.device_id == device_id)
            .map(|s| s.session_id.clone())
            .ok_or_else(|| "BUSY: 会话不存在，请先 open".to_string())?;

        let adapter = self
            .adapters
            .iter_mut()
            .find(|adapter| adapter.name == adapter_name)
            .ok_or_else(|| format!("unknown adapter '{adapter_name}'"))?;

        adapter.client.spi_transact(&session_id, write, read_len)
    }

    /// Close every open session best-effort, drop all clients (dropping
    /// `ChildTransport` kills the sidecar child processes) and clear the
    /// adapter/session vectors.
    pub fn shutdown(&mut self) {
        let sessions = std::mem::take(&mut self.sessions);
        for session in sessions {
            if let Some(adapter) = self
                .adapters
                .iter_mut()
                .find(|adapter| adapter.name == session.adapter)
            {
                if let Err(e) = adapter.client.close(&session.session_id) {
                    self.errors.push((
                        session.adapter.clone(),
                        format!("shutdown close failed: {e}"),
                    ));
                }
            }
        }

        self.adapters.clear();
        self.sessions.clear();
    }
}
