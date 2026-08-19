//! L0 plugin system: manifest parsing, capability whitelist and dependency
//! resolution.
//!
//! This module is intentionally Tauri-free; it only reads plugin manifests
//! from disk and keeps the loaded plugin state in memory.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Interface version implemented by this plugin manager.
pub const PLUGIN_API_VERSION: u32 = 1;
/// Version of the built-in `uni-base` virtual dependency.
pub const UNI_BASE_API_VERSION: &str = "1.0.0";

/// Plugin category, serialized in snake_case in manifests and IPC payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Adapter,
    Protocol,
    #[serde(rename = "chipdb")]
    ChipDb,
    Ui,
}

impl PluginKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PluginKind::Adapter => "adapter",
            PluginKind::Protocol => "protocol",
            PluginKind::ChipDb => "chipdb",
            PluginKind::Ui => "ui",
        }
    }
}

impl std::fmt::Display for PluginKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PluginKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "adapter" => Ok(PluginKind::Adapter),
            "protocol" => Ok(PluginKind::Protocol),
            "chipdb" => Ok(PluginKind::ChipDb),
            "ui" => Ok(PluginKind::Ui),
            other => Err(format!(
                "unknown plugin kind '{other}' (expected adapter, protocol, chipdb or ui)"
            )),
        }
    }
}

/// A named dependency with a semver requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dependency {
    pub name: String,
    pub requirement: String,
}

/// Manifest permissions. Defaults to full deny.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct PluginPermissions {
    pub serial: bool,
    pub usb: bool,
    pub network: bool,
    pub files: Vec<String>,
}

/// Declared SPI capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpiCapability {
    /// `(cs, sck, mosi, miso)` pin names.
    pub pins: Option<(String, String, String, String)>,
    pub max_frame: usize,
    pub max_freq_khz: u32,
}

/// Declared UART capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UartCapability {
    pub endpoint: Option<String>,
}

/// Declared VCC/IO power control capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerCapability {
    /// Supported output voltage range in millivolts, inclusive.
    pub range_mv: Option<(u32, u32)>,
}

/// Capability whitelist. The default is deny-all: everything `None`/`false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct CapabilitySet {
    pub spi: Option<SpiCapability>,
    pub uart: Option<UartCapability>,
    pub i2c: bool,
    pub gpio: bool,
    pub vcc_control: Option<PowerCapability>,
    pub wp_control: bool,
}

impl CapabilitySet {
    /// Intersect runtime-effective capabilities (`self`) with the manifest
    /// declared whitelist (`declared`).
    ///
    /// A capability is only exposed when both sides declare/support it. For
    /// nested ranges the intersection is computed; an empty intersection is an
    /// error because it means the adapter cannot safely satisfy the declared
    /// operating window.
    pub fn expose(self, declared: &CapabilitySet) -> Result<CapabilitySet, String> {
        let spi =
            match (self.spi, declared.spi.as_ref()) {
                (Some(eff), Some(decl)) => Some(SpiCapability {
                    pins: Some(eff.pins.clone().or_else(|| decl.pins.clone()).ok_or_else(
                        || "SPI capability: no pins available on either side".to_string(),
                    )?),
                    max_frame: intersect_limit(eff.max_frame, decl.max_frame),
                    max_freq_khz: intersect_limit_u32(eff.max_freq_khz, decl.max_freq_khz),
                }),
                _ => None,
            };

        let uart = match (self.uart, declared.uart.as_ref()) {
            (Some(eff), Some(decl)) => Some(UartCapability {
                endpoint: eff.endpoint.clone().or_else(|| decl.endpoint.clone()),
            }),
            _ => None,
        };

        let vcc_control = match (self.vcc_control, declared.vcc_control.as_ref()) {
            (Some(eff), Some(decl)) => {
                let range = match (eff.range_mv, decl.range_mv) {
                    (Some((eff_lo, eff_hi)), Some((decl_lo, decl_hi))) => {
                        let lo = eff_lo.max(decl_lo);
                        let hi = eff_hi.min(decl_hi);
                        if lo > hi {
                            return Err(format!(
                                "VCC control range intersection is empty: effective {eff_lo}-{eff_hi} mV vs declared {decl_lo}-{decl_hi} mV"
                            ));
                        }
                        Some((lo, hi))
                    }
                    _ => {
                        return Err(
                            "VCC control: both effective and declared ranges are required"
                                .to_string(),
                        );
                    }
                };
                Some(PowerCapability { range_mv: range })
            }
            _ => None,
        };

        Ok(CapabilitySet {
            spi,
            uart,
            i2c: self.i2c && declared.i2c,
            gpio: self.gpio && declared.gpio,
            vcc_control,
            wp_control: self.wp_control && declared.wp_control,
        })
    }
}

/// Pick the stricter (smaller) positive limit. A zero side means "not
/// reported"; in that case the other side is kept.
fn intersect_limit(eff: usize, decl: usize) -> usize {
    match (eff, decl) {
        (0, 0) => 0,
        (0, d) => d,
        (e, 0) => e,
        (e, d) => e.min(d),
    }
}

fn intersect_limit_u32(eff: u32, decl: u32) -> u32 {
    match (eff, decl) {
        (0, 0) => 0,
        (0, d) => d,
        (e, 0) => e,
        (e, d) => e.min(d),
    }
}

/// Parsed plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: semver::Version,
    pub plugin_api: u32,
    pub kind: PluginKind,
    pub entry: String,
    pub dependencies: Vec<Dependency>,
    pub permissions: PluginPermissions,
    pub capabilities: CapabilitySet,
    pub os: Vec<String>,
    pub arch: Vec<String>,
    pub app: Option<String>,
}

impl PluginManifest {
    pub fn parse(toml_text: &str) -> Result<Self, String> {
        let root: toml::Value = toml_text
            .parse()
            .map_err(|e| format!("invalid TOML: {e}"))?;
        let root = root
            .as_table()
            .ok_or_else(|| "manifest root must be a TOML table".to_string())?;

        let package = root
            .get("package")
            .and_then(|v| v.as_table())
            .ok_or_else(|| "missing [package] section".to_string())?;
        warn_unknown_fields(
            "[package]",
            package,
            &[
                "name",
                "version",
                "plugin_api",
                "kind",
                "entry",
                "os",
                "arch",
                "app",
            ],
        );

        let name =
            string_field(package, "name")?.ok_or_else(|| "missing package.name".to_string())?;
        let version = string_field(package, "version")?
            .ok_or_else(|| "missing package.version".to_string())?;
        let version = semver::Version::parse(&version)
            .map_err(|e| format!("invalid package.version '{version}': {e}"))?;
        let plugin_api = int_field(package, "plugin_api")?
            .ok_or_else(|| "missing package.plugin_api".to_string())?;
        let plugin_api = u32::try_from(plugin_api)
            .map_err(|_| "package.plugin_api must fit in u32".to_string())?;
        let kind =
            string_field(package, "kind")?.ok_or_else(|| "missing package.kind".to_string())?;
        let kind = PluginKind::from_str(&kind)?;
        let entry =
            string_field(package, "entry")?.ok_or_else(|| "missing package.entry".to_string())?;
        let os = string_array_field(package, "os")?;
        let arch = string_array_field(package, "arch")?;
        let app = string_field(package, "app")?;

        let dependencies = parse_dependencies(root)?;
        let permissions = parse_permissions(root)?;
        let capabilities = parse_capabilities(root)?;

        // The declared whitelist must be self-consistent: every declared
        // capability has to survive an intersection with itself. This is also
        // the same intersection the HAL will perform against runtime
        // capabilities.
        capabilities
            .clone()
            .expose(&capabilities)
            .map_err(|e| format!("invalid capabilities: {e}"))?;

        Ok(PluginManifest {
            name,
            version,
            plugin_api,
            kind,
            entry,
            dependencies,
            permissions,
            capabilities,
            os,
            arch,
            app,
        })
    }
}

/// A successfully parsed plugin directory.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub enabled: bool,
}

/// In-memory plugin registry.
#[derive(Debug, Default)]
pub struct PluginManager {
    pub plugins: Vec<LoadedPlugin>,
    /// `(plugin name or manifest path, error)` for skipped invalid plugins.
    pub errors: Vec<(String, String)>,
}

impl PluginManager {
    /// Scan `<root>/plugins/*/manifest.toml` (non-recursive). Directories
    /// without a manifest are ignored; invalid manifests are recorded in
    /// `errors` and skipped.
    pub fn load(root: &Path) -> Self {
        let plugins_dir = root.join("plugins");
        let mut plugins = Vec::new();
        let mut errors = Vec::new();
        let mut first_path_by_name: HashMap<String, String> = HashMap::new();

        let entries = match fs::read_dir(&plugins_dir) {
            Ok(entries) => entries,
            Err(_) => return PluginManager { plugins, errors },
        };

        for entry in entries.flatten() {
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("manifest.toml");
            if !manifest_path.is_file() {
                continue;
            }

            let manifest_path_text = manifest_path.display().to_string();
            let text = match fs::read_to_string(&manifest_path) {
                Ok(text) => text,
                Err(e) => {
                    errors.push((manifest_path_text, format!("failed to read manifest: {e}")));
                    continue;
                }
            };

            match PluginManifest::parse(&text) {
                Ok(manifest) => {
                    if let Some(first_path) = first_path_by_name.get(&manifest.name) {
                        errors.push((
                            manifest_path_text,
                            format!(
                                "duplicate plugin name '{}' (already loaded from {})",
                                manifest.name, first_path
                            ),
                        ));
                    } else {
                        first_path_by_name.insert(manifest.name.clone(), manifest_path_text);
                        plugins.push(LoadedPlugin {
                            manifest,
                            path: plugin_dir,
                            enabled: false,
                        });
                    }
                }
                Err(e) => errors.push((manifest_path_text, e)),
            }
        }

        PluginManager { plugins, errors }
    }

    /// Validate and resolve the dependency graph for `name`.
    ///
    /// `uni-base` is built-in and treated as available at
    /// [`UNI_BASE_API_VERSION`]. Other dependencies must reference loaded
    /// plugins and satisfy the semver requirement (`^`, `~`, exact, ...).
    ///
    /// Returns the resolved plugin names in dependency-first order, including
    /// `name` itself. A dependency cycle is reported as a readable path.
    pub fn resolve_dependencies(&self, name: &str) -> Result<Vec<String>, String> {
        let mut resolved = Vec::new();
        let mut visiting = Vec::new();
        let mut visited = HashSet::new();

        fn visit(
            manager: &PluginManager,
            name: &str,
            resolved: &mut Vec<String>,
            visiting: &mut Vec<String>,
            visited: &mut HashSet<String>,
        ) -> Result<(), String> {
            if visited.contains(name) {
                return Ok(());
            }
            if let Some(pos) = visiting.iter().position(|n| n == name) {
                let mut cycle = visiting[pos..].to_vec();
                cycle.push(name.to_string());
                return Err(format!("dependency cycle: {}", cycle.join(" -> ")));
            }

            visiting.push(name.to_string());

            let plugin = manager
                .plugins
                .iter()
                .find(|p| p.manifest.name == name)
                .ok_or_else(|| format!("unknown plugin '{name}'"))?;

            for dep in &plugin.manifest.dependencies {
                let requirement = semver::VersionReq::parse(&dep.requirement).map_err(|e| {
                    format!(
                        "plugin '{name}' has invalid version requirement '{}' for '{}': {e}",
                        dep.requirement, dep.name
                    )
                })?;

                if dep.name == "uni-base" {
                    let available = semver::Version::parse(UNI_BASE_API_VERSION)
                        .expect("UNI_BASE_API_VERSION must be valid semver");
                    if !requirement.matches(&available) {
                        return Err(format!(
                            "plugin '{name}' requires uni-base '{}' but built-in version is {UNI_BASE_API_VERSION}",
                            dep.requirement
                        ));
                    }
                } else {
                    let dep_plugin = manager
                        .plugins
                        .iter()
                        .find(|p| p.manifest.name == dep.name)
                        .ok_or_else(|| {
                            format!("plugin '{name}' has unknown dependency '{}'", dep.name)
                        })?;
                    if !requirement.matches(&dep_plugin.manifest.version) {
                        return Err(format!(
                            "plugin '{name}' requires {} '{}' but version {} is loaded",
                            dep.name, dep.requirement, dep_plugin.manifest.version
                        ));
                    }
                    visit(
                        manager,
                        &dep_plugin.manifest.name,
                        resolved,
                        visiting,
                        visited,
                    )?;
                }
            }

            visiting.pop();
            visited.insert(name.to_string());
            resolved.push(name.to_string());
            Ok(())
        }

        visit(self, name, &mut resolved, &mut visiting, &mut visited)?;
        Ok(resolved)
    }

    /// Enable a plugin after validating API, platform compatibility and
    /// dependencies. This only flips the in-memory flag.
    pub fn enable(&mut self, name: &str) -> Result<(), String> {
        let index = self
            .plugins
            .iter()
            .position(|p| p.manifest.name == name)
            .ok_or_else(|| format!("unknown plugin '{name}'"))?;

        {
            let plugin = &self.plugins[index];
            if plugin.manifest.plugin_api != PLUGIN_API_VERSION {
                return Err(format!(
                    "plugin '{name}' targets plugin_api {} but this host implements {PLUGIN_API_VERSION}",
                    plugin.manifest.plugin_api
                ));
            }
            if !plugin.manifest.os.is_empty()
                && !plugin
                    .manifest
                    .os
                    .iter()
                    .any(|os| os == std::env::consts::OS)
            {
                return Err(format!(
                    "plugin '{name}' is not compatible with OS '{}' (allowed: {})",
                    std::env::consts::OS,
                    plugin.manifest.os.join(", ")
                ));
            }
            if !plugin.manifest.arch.is_empty()
                && !plugin
                    .manifest
                    .arch
                    .iter()
                    .any(|arch| arch == std::env::consts::ARCH)
            {
                return Err(format!(
                    "plugin '{name}' is not compatible with arch '{}' (allowed: {})",
                    std::env::consts::ARCH,
                    plugin.manifest.arch.join(", ")
                ));
            }
        }

        self.resolve_dependencies(name)?;
        self.plugins[index].enabled = true;
        Ok(())
    }

    /// Disable a plugin. In-memory state only.
    pub fn disable(&mut self, name: &str) -> Result<(), String> {
        let plugin = self
            .plugins
            .iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| format!("unknown plugin '{name}'"))?;
        plugin.enabled = false;
        Ok(())
    }
}

fn parse_dependencies(
    root: &toml::map::Map<String, toml::Value>,
) -> Result<Vec<Dependency>, String> {
    let mut dependencies = Vec::new();
    if let Some(value) = root.get("dependencies") {
        let table = value
            .as_table()
            .ok_or_else(|| "[dependencies] must be a table".to_string())?;
        for (name, req) in table {
            let requirement = req
                .as_str()
                .ok_or_else(|| format!("dependency '{name}' must have a string requirement"))?;
            dependencies.push(Dependency {
                name: name.clone(),
                requirement: requirement.to_string(),
            });
        }
    }
    Ok(dependencies)
}

fn parse_permissions(
    root: &toml::map::Map<String, toml::Value>,
) -> Result<PluginPermissions, String> {
    let mut permissions = PluginPermissions::default();
    if let Some(value) = root.get("permissions") {
        let table = value
            .as_table()
            .ok_or_else(|| "[permissions] must be a table".to_string())?;
        warn_unknown_fields(
            "[permissions]",
            table,
            &["serial", "usb", "network", "files"],
        );
        permissions.serial = bool_field(table, "serial")?.unwrap_or(false);
        permissions.usb = bool_field(table, "usb")?.unwrap_or(false);
        permissions.network = bool_field(table, "network")?.unwrap_or(false);
        permissions.files = string_array_field(table, "files")?;
    }
    Ok(permissions)
}

fn parse_capabilities(root: &toml::map::Map<String, toml::Value>) -> Result<CapabilitySet, String> {
    let mut capabilities = CapabilitySet::default();
    let caps_table = match root.get("capabilities") {
        None => return Ok(capabilities),
        Some(value) => value
            .as_table()
            .ok_or_else(|| "[capabilities] must be a table".to_string())?,
    };

    for (key, value) in caps_table {
        match key.as_str() {
            "spi" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| "[capabilities.spi] must be a table".to_string())?;
                capabilities.spi = parse_spi(table)?;
            }
            "uart" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| "[capabilities.uart] must be a table".to_string())?;
                capabilities.uart = parse_uart(table)?;
            }
            "i2c" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| "[capabilities.i2c] must be a table".to_string())?;
                capabilities.i2c = parse_bool_capability("capabilities.i2c", table)?;
            }
            "gpio" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| "[capabilities.gpio] must be a table".to_string())?;
                capabilities.gpio = parse_bool_capability("capabilities.gpio", table)?;
            }
            "vcc_control" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| "[capabilities.vcc_control] must be a table".to_string())?;
                capabilities.vcc_control = parse_vcc(table)?;
            }
            "wp_control" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| "[capabilities.wp_control] must be a table".to_string())?;
                capabilities.wp_control = parse_bool_capability("capabilities.wp_control", table)?;
            }
            other => eprintln!("[plugin] unknown [capabilities.{other}] table ignored"),
        }
    }

    Ok(capabilities)
}

fn parse_spi(table: &toml::map::Map<String, toml::Value>) -> Result<Option<SpiCapability>, String> {
    warn_unknown_fields(
        "capabilities.spi",
        table,
        &["enabled", "pins", "max_frame", "max_freq_khz"],
    );
    let enabled = bool_field(table, "enabled")?.unwrap_or(true);
    if !enabled {
        return Ok(None);
    }

    let pins = table
        .get("pins")
        .and_then(|v| v.as_table())
        .ok_or_else(|| "capabilities.spi.pins is required when enabled".to_string())?;
    let cs = pin_field(pins, "cs")?;
    let sck = pin_field(pins, "sck")?;
    let mosi = pin_field(pins, "mosi")?;
    let miso = pin_field(pins, "miso")?;

    let max_frame = int_field(table, "max_frame")?
        .ok_or_else(|| "capabilities.spi.max_frame is required when enabled".to_string())?;
    let max_frame = usize::try_from(max_frame)
        .map_err(|_| "capabilities.spi.max_frame must be a positive integer".to_string())?;
    if max_frame == 0 {
        return Err("capabilities.spi.max_frame must be > 0".to_string());
    }

    let max_freq_khz = int_field(table, "max_freq_khz")?
        .ok_or_else(|| "capabilities.spi.max_freq_khz is required when enabled".to_string())?;
    let max_freq_khz = u32::try_from(max_freq_khz)
        .map_err(|_| "capabilities.spi.max_freq_khz must be a positive integer".to_string())?;
    if max_freq_khz == 0 {
        return Err("capabilities.spi.max_freq_khz must be > 0".to_string());
    }

    Ok(Some(SpiCapability {
        pins: Some((cs, sck, mosi, miso)),
        max_frame,
        max_freq_khz,
    }))
}

fn parse_uart(
    table: &toml::map::Map<String, toml::Value>,
) -> Result<Option<UartCapability>, String> {
    warn_unknown_fields("capabilities.uart", table, &["enabled", "endpoint"]);
    let enabled = bool_field(table, "enabled")?.unwrap_or(true);
    if !enabled {
        return Ok(None);
    }

    let endpoint = string_field(table, "endpoint")?
        .ok_or_else(|| "capabilities.uart.endpoint is required when enabled".to_string())?;
    Ok(Some(UartCapability {
        endpoint: Some(endpoint),
    }))
}

fn parse_bool_capability(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
) -> Result<bool, String> {
    warn_unknown_fields(path, table, &["enabled"]);
    // Default deny: a table without `enabled = true` does not grant access.
    bool_field(table, "enabled").map(|enabled| enabled.unwrap_or(false))
}

fn parse_vcc(
    table: &toml::map::Map<String, toml::Value>,
) -> Result<Option<PowerCapability>, String> {
    warn_unknown_fields("capabilities.vcc_control", table, &["enabled", "range_mv"]);
    let enabled = bool_field(table, "enabled")?.unwrap_or(false);
    if !enabled {
        return Ok(None);
    }

    let range = table
        .get("range_mv")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "capabilities.vcc_control.range_mv is required when enabled".to_string())?;
    if range.len() != 2 {
        return Err("capabilities.vcc_control.range_mv must be [min_mv, max_mv]".to_string());
    }
    let lo = range[0]
        .as_integer()
        .ok_or_else(|| "capabilities.vcc_control.range_mv values must be integers".to_string())?;
    let hi = range[1]
        .as_integer()
        .ok_or_else(|| "capabilities.vcc_control.range_mv values must be integers".to_string())?;
    if lo < 0 || hi < 0 || lo > hi {
        return Err(format!(
            "capabilities.vcc_control.range_mv must be [min, max] with 0 <= min <= max (got [{lo}, {hi}])"
        ));
    }
    let lo =
        u32::try_from(lo).map_err(|_| "capabilities.vcc_control range too large".to_string())?;
    let hi =
        u32::try_from(hi).map_err(|_| "capabilities.vcc_control range too large".to_string())?;

    Ok(Some(PowerCapability {
        range_mv: Some((lo, hi)),
    }))
}

fn string_field(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("field '{key}' must be a string")),
    }
}

fn int_field(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<i64>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_integer()
            .map(Some)
            .ok_or_else(|| format!("field '{key}' must be an integer")),
    }
}

fn bool_field(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<bool>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("field '{key}' must be a boolean")),
    }
}

fn string_array_field(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("field '{key}' must be an array of strings"))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("field '{key}' must be an array of strings"))
        })
        .collect()
}

fn pin_field(pins: &toml::map::Map<String, toml::Value>, key: &str) -> Result<String, String> {
    let value = pins
        .get(key)
        .ok_or_else(|| format!("capabilities.spi.pins.{key} is required when enabled"))?;
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("capabilities.spi.pins.{key} must be a string"))
}

fn warn_unknown_fields(path: &str, table: &toml::map::Map<String, toml::Value>, known: &[&str]) {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            eprintln!("[plugin] unknown field '{path}.{key}' ignored");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const MINIMAL: &str = r#"
[package]
name = "vnd.example.minimal"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"
"#;

    fn test_root(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "uniprogrammer-plugin-test-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_manifest(root: &Path, plugin_dir: &str, content: &str) {
        let dir = root.join("plugins").join(plugin_dir);
        fs::create_dir_all(&dir).expect("create test plugin dir");
        fs::write(dir.join("manifest.toml"), content).expect("write test manifest");
    }

    fn loaded_plugin(name: &str, version: &str, dependencies: Vec<Dependency>) -> LoadedPlugin {
        LoadedPlugin {
            manifest: PluginManifest {
                name: name.to_string(),
                version: semver::Version::parse(version).expect("valid test version"),
                plugin_api: PLUGIN_API_VERSION,
                kind: PluginKind::Adapter,
                entry: "plugin.exe".to_string(),
                dependencies,
                permissions: PluginPermissions::default(),
                capabilities: CapabilitySet::default(),
                os: Vec::new(),
                arch: Vec::new(),
                app: None,
            },
            path: PathBuf::from(name),
            enabled: false,
        }
    }

    #[test]
    fn parses_full_manifest() {
        let manifest = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.spi-programmer"
version = "1.2.3"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"
os = ["windows", "linux"]
arch = ["x86_64"]
app = "^0.3"

[dependencies]
"uni-base" = "^1"
"vnd.example.protocol" = "~1.0"

[permissions]
serial = false
usb = true
network = false
files = ["chips.json", "profiles/*.toml"]

[capabilities.spi]
pins = { cs = "CS1", sck = "SCK", mosi = "MOSI", miso = "MISO" }
max_frame = 4092
max_freq_khz = 60000

[capabilities.uart]
endpoint = "UART1"

[capabilities.i2c]
enabled = true

[capabilities.gpio]
enabled = false

[capabilities.vcc_control]
enabled = true
range_mv = [1800, 3300]

[capabilities.wp_control]
enabled = true

[unknown_top_level]
foo = "bar"
"#,
        )
        .expect("full manifest should parse");

        assert_eq!(manifest.name, "vnd.example.spi-programmer");
        assert_eq!(manifest.version, semver::Version::new(1, 2, 3));
        assert_eq!(manifest.plugin_api, 1);
        assert_eq!(manifest.kind, PluginKind::Adapter);
        assert_eq!(manifest.entry, "plugin.exe");
        assert_eq!(manifest.os, vec!["windows", "linux"]);
        assert_eq!(manifest.arch, vec!["x86_64"]);
        assert_eq!(manifest.app.as_deref(), Some("^0.3"));
        assert_eq!(manifest.dependencies.len(), 2);
        assert!(manifest.permissions.usb);
        assert!(!manifest.permissions.serial);
        assert!(!manifest.permissions.network);
        assert_eq!(manifest.permissions.files.len(), 2);

        let spi = manifest.capabilities.spi.as_ref().expect("spi declared");
        assert_eq!(
            spi.pins,
            Some((
                "CS1".to_string(),
                "SCK".to_string(),
                "MOSI".to_string(),
                "MISO".to_string()
            ))
        );
        assert_eq!(spi.max_frame, 4092);
        assert_eq!(spi.max_freq_khz, 60000);
        assert_eq!(
            manifest
                .capabilities
                .uart
                .as_ref()
                .unwrap()
                .endpoint
                .as_deref(),
            Some("UART1")
        );
        assert!(manifest.capabilities.i2c);
        assert!(!manifest.capabilities.gpio);
        assert_eq!(
            manifest.capabilities.vcc_control.as_ref().unwrap().range_mv,
            Some((1800, 3300))
        );
        assert!(manifest.capabilities.wp_control);
    }

    #[test]
    fn missing_capabilities_default_deny() {
        let manifest = PluginManifest::parse(MINIMAL).expect("minimal manifest should parse");
        let caps = manifest.capabilities;
        assert!(caps.spi.is_none());
        assert!(caps.uart.is_none());
        assert!(!caps.i2c);
        assert!(!caps.gpio);
        assert!(caps.vcc_control.is_none());
        assert!(!caps.wp_control);
        assert!(manifest.dependencies.is_empty());
        assert!(!manifest.permissions.serial);
        assert!(!manifest.permissions.usb);
        assert!(!manifest.permissions.network);
        assert!(manifest.permissions.files.is_empty());
    }

    #[test]
    fn enabled_false_capability_is_valid_and_none() {
        let manifest = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.disabled"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"

[capabilities.spi]
enabled = false

[capabilities.uart]
enabled = false

[capabilities.i2c]
enabled = false

[capabilities.gpio]
enabled = false

[capabilities.vcc_control]
enabled = false

[capabilities.wp_control]
enabled = false
"#,
        )
        .expect("disabled capabilities should parse");

        assert!(manifest.capabilities.spi.is_none());
        assert!(manifest.capabilities.uart.is_none());
        assert!(!manifest.capabilities.i2c);
        assert!(!manifest.capabilities.gpio);
        assert!(manifest.capabilities.vcc_control.is_none());
        assert!(!manifest.capabilities.wp_control);
    }

    #[test]
    fn enabled_capability_without_required_limits_is_error() {
        let err = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.bad-spi"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"

[capabilities.spi]
pins = { cs = "CS1", sck = "SCK", mosi = "MOSI", miso = "MISO" }
"#,
        )
        .expect_err("SPI without limits must fail");
        assert!(err.contains("max_frame"), "unexpected error: {err}");

        let err = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.bad-vcc"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"

[capabilities.vcc_control]
enabled = true
"#,
        )
        .expect_err("VCC without range must fail");
        assert!(err.contains("range_mv"), "unexpected error: {err}");
    }

    #[test]
    fn malformed_manifest_is_recorded_and_load_continues() {
        let root = test_root("load");
        write_manifest(&root, "broken", "[package]\nname = 42\n");
        write_manifest(&root, "good", MINIMAL);

        let manager = PluginManager::load(&root);

        assert_eq!(manager.plugins.len(), 1);
        assert_eq!(manager.plugins[0].manifest.name, "vnd.example.minimal");
        assert!(!manager.plugins[0].enabled);
        assert_eq!(manager.errors.len(), 1);
        assert!(manager.errors[0].0.contains("broken"));
        assert!(manager.errors[0].1.contains("name"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicate_plugin_names_are_a_load_error() {
        let root = test_root("duplicate");
        write_manifest(&root, "first", MINIMAL);
        write_manifest(&root, "second", MINIMAL);

        let manager = PluginManager::load(&root);
        assert_eq!(manager.plugins.len(), 1);
        assert_eq!(manager.errors.len(), 1);
        assert!(manager.errors[0].1.contains("duplicate plugin name"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dependency_resolution_uni_base_unknown_and_unsatisfied() {
        let manager = PluginManager {
            plugins: vec![
                loaded_plugin(
                    "only-uni-base",
                    "1.0.0",
                    vec![Dependency {
                        name: "uni-base".to_string(),
                        requirement: "^1".to_string(),
                    }],
                ),
                loaded_plugin(
                    "unknown-dep",
                    "1.0.0",
                    vec![Dependency {
                        name: "vnd.missing".to_string(),
                        requirement: "^1".to_string(),
                    }],
                ),
                loaded_plugin(
                    "unsat-uni-base",
                    "1.0.0",
                    vec![Dependency {
                        name: "uni-base".to_string(),
                        requirement: "^2".to_string(),
                    }],
                ),
                loaded_plugin(
                    "dep-version",
                    "1.0.0",
                    vec![Dependency {
                        name: "only-uni-base".to_string(),
                        requirement: "~0.9".to_string(),
                    }],
                ),
            ],
            errors: Vec::new(),
        };

        let resolved = manager
            .resolve_dependencies("only-uni-base")
            .expect("uni-base ^1");
        assert_eq!(resolved, vec!["only-uni-base"]);

        let err = manager
            .resolve_dependencies("unknown-dep")
            .expect_err("unknown dependency must fail");
        assert!(err.contains("unknown dependency 'vnd.missing'"), "{err}");

        let err = manager
            .resolve_dependencies("unsat-uni-base")
            .expect_err("unsatisfied uni-base must fail");
        assert!(err.contains("requires uni-base '^2'"), "{err}");

        let err = manager
            .resolve_dependencies("dep-version")
            .expect_err("unsatisfied plugin version must fail");
        assert!(err.contains("requires only-uni-base '~0.9'"), "{err}");
    }

    #[test]
    fn dependency_cycle_is_reported() {
        let manager = PluginManager {
            plugins: vec![
                loaded_plugin(
                    "a",
                    "1.0.0",
                    vec![Dependency {
                        name: "b".to_string(),
                        requirement: "^1".to_string(),
                    }],
                ),
                loaded_plugin(
                    "b",
                    "1.0.0",
                    vec![Dependency {
                        name: "a".to_string(),
                        requirement: "^1".to_string(),
                    }],
                ),
            ],
            errors: Vec::new(),
        };

        let err = manager
            .resolve_dependencies("a")
            .expect_err("cycle must fail");
        assert!(err.contains("dependency cycle"), "{err}");
    }

    #[test]
    fn capability_intersection_works() {
        let declared = CapabilitySet {
            spi: Some(SpiCapability {
                pins: Some((
                    "CS1".to_string(),
                    "SCK".to_string(),
                    "MOSI".to_string(),
                    "MISO".to_string(),
                )),
                max_frame: 4092,
                max_freq_khz: 60000,
            }),
            uart: Some(UartCapability {
                endpoint: Some("UART1".to_string()),
            }),
            i2c: true,
            gpio: true,
            vcc_control: Some(PowerCapability {
                range_mv: Some((1800, 3300)),
            }),
            wp_control: true,
        };

        let effective = CapabilitySet {
            spi: Some(SpiCapability {
                pins: Some((
                    "CS0".to_string(),
                    "SCK".to_string(),
                    "MOSI".to_string(),
                    "MISO".to_string(),
                )),
                max_frame: 2048,
                max_freq_khz: 30000,
            }),
            uart: Some(UartCapability {
                endpoint: Some("UART0".to_string()),
            }),
            i2c: true,
            gpio: false,
            vcc_control: Some(PowerCapability {
                range_mv: Some((2000, 4000)),
            }),
            wp_control: true,
        };

        let exposed = effective.expose(&declared).expect("ranges intersect");

        let spi = exposed.spi.as_ref().expect("spi exposed");
        assert_eq!(spi.max_frame, 2048);
        assert_eq!(spi.max_freq_khz, 30000);
        assert_eq!(spi.pins.as_ref().unwrap().0, "CS0");
        assert_eq!(
            exposed.uart.as_ref().unwrap().endpoint.as_deref(),
            Some("UART0")
        );
        assert!(exposed.i2c);
        assert!(!exposed.gpio);
        assert_eq!(
            exposed.vcc_control.as_ref().unwrap().range_mv,
            Some((2000, 3300))
        );
        assert!(exposed.wp_control);
    }

    #[test]
    fn capability_intersection_rejects_empty_vcc_range() {
        let declared = CapabilitySet {
            vcc_control: Some(PowerCapability {
                range_mv: Some((1800, 2000)),
            }),
            ..CapabilitySet::default()
        };
        let effective = CapabilitySet {
            vcc_control: Some(PowerCapability {
                range_mv: Some((2500, 3000)),
            }),
            ..CapabilitySet::default()
        };

        let err = effective
            .expose(&declared)
            .expect_err("empty range must fail");
        assert!(
            err.contains("VCC control range intersection is empty"),
            "{err}"
        );
    }
}
