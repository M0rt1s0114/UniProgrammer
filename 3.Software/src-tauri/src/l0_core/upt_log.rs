//! `upt.log`: L0 text logging pipeline.
//!
//! There is exactly one logger instance for the whole process. It writes to
//! stderr (visible when the debug console is enabled) and, when a log file
//! path is configured, appends the same line to `logs/uniprog.log`.
//!
//! Later this module becomes part of the `upt-base` prelude; business modules
//! will use it through `use upt_base::log` instead of owning their own log
//! channels.

use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Maximum log file size before it is rotated (truncated) on startup.
const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

struct UptLogger {
    level: Level,
    console: bool,
    file: Option<Mutex<File>>,
}

static LOGGER: OnceLock<UptLogger> = OnceLock::new();

/// Initialise the process-wide logger.
///
/// `level` filters messages below it; `console` mirrors every accepted line
/// to stderr; `file_path` additionally appends lines to a text file. The log
/// file is truncated once when it exceeds [`MAX_LOG_FILE_BYTES`].
pub fn init(level: Level, console: bool, file_path: Option<PathBuf>) -> Result<(), String> {
    let file = match file_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("创建日志目录失败 {}: {}", parent.display(), err))?;
            }
            if fs::metadata(&path)
                .map(|metadata| metadata.len() > MAX_LOG_FILE_BYTES)
                .unwrap_or(false)
            {
                let _ = fs::remove_file(&path);
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|err| format!("打开日志文件失败 {}: {}", path.display(), err))?;
            Some(Mutex::new(file))
        }
        None => None,
    };

    let logger = UptLogger {
        level,
        console,
        file,
    };
    let _ = LOGGER.set(logger);
    Ok(())
}

pub fn log(level: Level, message: &str) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    if level < logger.level {
        return;
    }

    let line = format!(
        "{} [{}] {message}",
        Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        level.as_str()
    );

    if logger.console {
        eprintln!("{line}");
    }
    if let Some(file) = &logger.file {
        if let Ok(mut file) = file.lock() {
            let _ = writeln!(file, "{line}");
        }
    }
}

pub fn info(message: &str) {
    log(Level::Info, message);
}

pub fn warn(message: &str) {
    log(Level::Warn, message);
}

pub fn error(message: &str) {
    log(Level::Error, message);
}

pub fn debug(message: &str) {
    log(Level::Debug, message);
}

/// Check whether the global logger has already been initialised.
pub fn is_initialised() -> bool {
    LOGGER.get().is_some()
}

/// Rotate (truncate) the currently configured log file, if any.
pub fn rotate_file() -> Result<(), String> {
    let Some(logger) = LOGGER.get() else {
        return Err("upt.log has not been initialised".to_string());
    };
    let Some(file) = &logger.file else {
        return Ok(());
    };
    let file = file
        .lock()
        .map_err(|err| format!("锁定日志文件失败: {err}"))?;
    file.set_len(0)
        .map_err(|err| format!("清空日志文件失败: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_order_filters_debug_by_default() {
        assert!(Level::Debug < Level::Info);
        assert!(Level::Info < Level::Warn);
        assert!(Level::Warn < Level::Error);
    }
}
