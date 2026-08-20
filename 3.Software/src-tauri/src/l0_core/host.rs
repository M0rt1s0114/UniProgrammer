//! `upt-base` host API.
//!
//! The L0 core owns this trait; UI shells (Tauri today, Slint later) inject a
//! concrete implementation at startup. Business modules only depend on this
//! trait, never on a specific UI framework.
//!
//! The current [`HostContext`] is the standalone implementation used by the
//! existing Tauri shell. When the command layer is fully ported, Tauri will
//! provide its own implementation and only `HostApi` remains in L0.

use std::path::PathBuf;

use super::upt_log::{self, Level};

pub trait HostApi: Send + Sync {
    /// Application root directory. In portable builds this is the executable
    /// directory; in debug builds it is the process working directory.
    fn root_dir(&self) -> PathBuf;

    /// Write one message through the process-wide `upt.log` pipeline.
    fn log(&self, level: Level, message: &str);
}

/// Standalone host context used before a framework-specific host is injected.
#[derive(Debug, Clone)]
pub struct HostContext {
    root_dir: PathBuf,
}

impl HostContext {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }
}

impl HostApi for HostContext {
    fn root_dir(&self) -> PathBuf {
        self.root_dir.clone()
    }

    fn log(&self, level: Level, message: &str) {
        upt_log::log(level, message);
    }
}
