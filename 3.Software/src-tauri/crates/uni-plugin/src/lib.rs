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
/// L1 required plugin set. These plugins are implicit and must be present at
/// boot; they are compile-time builtins represented by shipped manifests.
pub const REQUIRED_PLUGIN_NAMES: &[&str] = &[
    "uni.tauri",
    "uni.hal",
    "uni.chipdb",
    "uni.tauri.hexview",
    "uni.proto",
];

/// Result of the boot-time required-plugin check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BootCheck {
    /// Required plugin names that were not loaded at all.
    pub missing: Vec<String>,
    /// Required plugin names whose manifest was loaded but recorded as invalid
    /// (parse error or duplicate name).
    pub invalid: Vec<String>,
}

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

/// Plugin layer semantics, serialized in snake_case in manifests and IPC
/// payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginLayer {
    /// Core plugin required by the application; always enabled.
    Required,
    /// Optional plugin loaded on a cold start path.
    #[default]
    Cold,
    /// Optional plugin loaded on a hot/live path.
    Hot,
}

impl PluginLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            PluginLayer::Required => "required",
            PluginLayer::Cold => "cold",
            PluginLayer::Hot => "hot",
        }
    }
}

impl std::fmt::Display for PluginLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PluginLayer {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "required" => Ok(PluginLayer::Required),
            "cold" => Ok(PluginLayer::Cold),
            "hot" => Ok(PluginLayer::Hot),
            other => Err(format!(
                "unknown plugin layer '{other}' (expected required, cold or hot)"
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

/// A compiled-in module of the `uni-base` registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuiltinModule {
    pub name: String,
    pub version: String,
    pub interface_version: u32,
    pub capabilities: CapabilitySet,
    pub description: String,
}

/// Compiled-in `uni-base` modules.
///
/// This registry is descriptive for now: it lets dependency resolution
/// recognize the capabilities built into the current program, without running
/// them as separate plugin processes.
pub fn builtin_modules() -> Vec<BuiltinModule> {
    fn module(name: &str, capabilities: CapabilitySet, description: &str) -> BuiltinModule {
        BuiltinModule {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            interface_version: PLUGIN_API_VERSION,
            capabilities,
            description: description.to_string(),
        }
    }

    fn spi_capability() -> CapabilitySet {
        CapabilitySet {
            spi: Some(SpiCapability {
                pins: None,
                max_frame: 4092,
                max_freq_khz: 60_000,
            }),
            ..CapabilitySet::default()
        }
    }

    vec![
        module(
            "uni.core",
            CapabilitySet::default(),
            "应用状态、操作流水线与插件管理器核心",
        ),
        module(
            "uni.hal",
            CapabilitySet::default(),
            "硬件抽象层路由与能力协商",
        ),
        module(
            "uni.chipdb",
            CapabilitySet::default(),
            "芯片数据库查询、导入与统计",
        ),
        module(
            "uni.tauri",
            CapabilitySet::default(),
            "Tauri UI 壳（主窗口宿主）",
        ),
        module(
            "uni.tauri.hexview",
            CapabilitySet::default(),
            "Tauri UI 的十六进制视图贡献页",
        ),
        module(
            "uni.proto.spi-nor",
            CapabilitySet::default(),
            "SPI NOR Flash 协议",
        ),
        module(
            "uni.proto.spi-nand",
            CapabilitySet::default(),
            "SPI NAND Flash 协议",
        ),
        module(
            "uni.proto.spi-eeprom",
            CapabilitySet::default(),
            "SPI EEPROM 协议",
        ),
        module(
            "uni.proto.data45",
            CapabilitySet::default(),
            "DataFlash AT45 协议",
        ),
        module("uni.proto.i2c", CapabilitySet::default(), "I2C 芯片协议"),
        module(
            "uni.proto.microwire",
            CapabilitySet::default(),
            "MicroWire 芯片协议",
        ),
        module(
            "uni.hal.ch34x",
            spi_capability(),
            "CH341A / CH347T / CH347F 编程器适配器",
        ),
        module("uni.hal.serprog", spi_capability(), "serprog 编程器适配器"),
    ]
}

/// Look up a built-in module version by exact name.
pub fn builtin_version(name: &str) -> Option<&'static str> {
    match name {
        "uni.core"
        | "uni.tauri"
        | "uni.hal"
        | "uni.chipdb"
        | "uni.tauri.hexview"
        | "uni.proto.spi-nor"
        | "uni.proto.spi-nand"
        | "uni.proto.spi-eeprom"
        | "uni.proto.data45"
        | "uni.proto.i2c"
        | "uni.proto.microwire"
        | "uni.hal.ch34x"
        | "uni.hal.serprog" => Some("1.0.0"),
        _ => None,
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
    pub layer: PluginLayer,
    pub entry: String,
    pub dependencies: Vec<Dependency>,
    pub permissions: PluginPermissions,
    pub capabilities: CapabilitySet,
    pub os: Vec<String>,
    pub arch: Vec<String>,
    pub app: Option<String>,
    pub provider: Option<String>,
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
                "layer",
                "entry",
                "provider",
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
        let layer = match string_field(package, "layer")? {
            Some(layer) => PluginLayer::from_str(&layer)?,
            None => default_layer(kind),
        };
        let entry =
            string_field(package, "entry")?.ok_or_else(|| "missing package.entry".to_string())?;
        let provider = string_field(package, "provider")?;
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
            layer,
            entry,
            dependencies,
            permissions,
            capabilities,
            os,
            arch,
            app,
            provider,
        })
    }
}

/// A successfully parsed plugin directory.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub enabled: bool,
    /// True when the plugin was loaded from `plugins/builtin/` and is part of
    /// the trusted built-in set shipped with the app.
    pub builtin: bool,
}

/// Return the manifest files that exist in `dir`, in preferred load order:
/// `unipkg.toml` first, then the legacy `manifest.toml`.
///
/// When both files exist both paths are returned (`unipkg.toml` first) and
/// callers should use the first one. When only one exists it is returned
/// alone; when neither exists the vector is empty.
pub fn manifest_candidates(dir: &Path) -> Vec<PathBuf> {
    ["unipkg.toml", "manifest.toml"]
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

/// Persisted enable/disable state for non-required plugins.
///
/// L2 cold plugins are loaded at startup, so the user's choice must survive a
/// restart. The file lives next to the `plugins/` scan root:
/// `<root>/plugin-state.toml`.
pub fn plugin_state_path(root: &Path) -> PathBuf {
    root.join("plugin-state.toml")
}

/// Read `<root>/plugin-state.toml` as a `plugin name -> enabled` map.
///
/// A missing or unreadable file yields an empty map (everything keeps its
/// default); malformed entries are skipped best-effort.
pub fn load_plugin_state(root: &Path) -> HashMap<String, bool> {
    let path = plugin_state_path(root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return HashMap::new(),
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        eprintln!(
            "[uni-plugin] ignoring malformed plugin state file {}",
            path.display()
        );
        return HashMap::new();
    };

    let mut state = HashMap::new();
    if let Some(table) = value.get("plugins").and_then(toml::Value::as_table) {
        for (name, enabled) in table {
            if let Some(enabled) = enabled.as_bool() {
                state.insert(name.clone(), enabled);
            } else {
                eprintln!(
                    "[uni-plugin] ignoring non-boolean enabled value for plugin '{name}' in {}",
                    path.display()
                );
            }
        }
    }
    state
}

/// Persist the enabled flag of every non-required plugin.
pub fn save_plugin_state(root: &Path, manager: &PluginManager) -> Result<(), String> {
    let mut plugins = toml::map::Map::new();
    for plugin in &manager.plugins {
        if plugin.manifest.layer != PluginLayer::Required {
            plugins.insert(
                plugin.manifest.name.clone(),
                toml::Value::Boolean(plugin.enabled),
            );
        }
    }

    let mut root_table = toml::map::Map::new();
    root_table.insert("plugins".to_string(), toml::Value::Table(plugins));
    let text = toml::to_string_pretty(&toml::Value::Table(root_table))
        .map_err(|e| format!("failed to encode plugin state: {e}"))?;
    fs::write(plugin_state_path(root), text)
        .map_err(|e| format!("failed to write plugin state: {e}"))
}

/// In-memory plugin registry.
#[derive(Debug, Default)]
pub struct PluginManager {
    pub plugins: Vec<LoadedPlugin>,
    /// `(plugin name or manifest path, error)` for skipped invalid plugins.
    pub errors: Vec<(String, String)>,
}

impl PluginManager {
    /// Scan `<root>/plugins/*` (non-recursive) and
    /// `<root>/plugins/builtin/*` (non-recursive) for `unipkg.toml` or the
    /// legacy `manifest.toml`; `unipkg.toml` is preferred when both exist.
    /// Directories without a manifest are ignored; invalid manifests are
    /// recorded in `errors` and skipped. Duplicate plugin names are detected
    /// across both scan roots.
    pub fn load(root: &Path) -> Self {
        let plugins_dir = root.join("plugins");
        let mut plugins = Vec::new();
        let mut errors = Vec::new();
        let mut first_path_by_name: HashMap<String, String> = HashMap::new();
        let enabled_state = load_plugin_state(root);

        if fs::read_dir(&plugins_dir).is_err() {
            return PluginManager { plugins, errors };
        }

        let builtin_dir = plugins_dir.join("builtin");
        for scan_dir in [plugins_dir.clone(), builtin_dir.clone()] {
            let is_builtin_scan = scan_dir == builtin_dir;
            let entries = match fs::read_dir(&scan_dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let plugin_dir = entry.path();
                if !plugin_dir.is_dir() {
                    continue;
                }
                let manifest_path = match manifest_candidates(&plugin_dir).into_iter().next() {
                    Some(path) => path,
                    None => continue,
                };

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
                            let enabled = if manifest.layer == PluginLayer::Required {
                                true
                            } else if is_builtin_scan {
                                // Built-in plugins are part of the shipped
                                // product and start enabled; the persisted
                                // state may opt them out explicitly.
                                enabled_state.get(&manifest.name).copied().unwrap_or(true)
                            } else {
                                // Third-party cold/hot plugins stay disabled
                                // until the user enables them in the plugin
                                // manager.
                                enabled_state.get(&manifest.name).copied().unwrap_or(false)
                            };

                            first_path_by_name.insert(manifest.name.clone(), manifest_path_text);
                            plugins.push(LoadedPlugin {
                                manifest,
                                path: plugin_dir,
                                enabled,
                                builtin: is_builtin_scan,
                            });
                        }
                    }
                    Err(e) => errors.push((manifest_path_text, e)),
                }
            }
        }

        PluginManager { plugins, errors }
    }

    /// Check the L1 required plugin set against the loaded manifests.
    ///
    /// A required name is reported in `missing` when no plugin with that name
    /// was loaded and no matching error was recorded. When `errors` contains
    /// an entry for its name or manifest path (for example a parse error or a
    /// duplicate name) it is reported in `invalid`.
    pub fn boot_check(&self) -> BootCheck {
        let mut missing = Vec::new();
        let mut invalid = Vec::new();

        for required in REQUIRED_PLUGIN_NAMES {
            let has_error = self
                .errors
                .iter()
                .any(|(key, _)| error_key_matches(key, required));
            if has_error {
                invalid.push(required.to_string());
                continue;
            }

            let loaded = self.plugins.iter().any(|p| p.manifest.name == *required);
            if !loaded {
                missing.push(required.to_string());
            }
        }

        BootCheck { missing, invalid }
    }

    /// Validate and resolve the dependency graph for `name`.
    ///
    /// Dependency names are first looked up in the built-in `uni-base`
    /// registry (see [`builtin_modules`]) and then among loaded plugins.
    /// `uni-base` itself remains a virtual meta dependency: it is checked
    /// against [`UNI_BASE_API_VERSION`] and expands implicitly to the whole
    /// builtin registry, so no individual builtin entries are appended to the
    /// returned list for a `uni-base` dependency.
    ///
    /// Returns the resolved loaded-plugin names in dependency-first order,
    /// including `name` itself. A dependency cycle is reported as a readable
    /// path.
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

            // Builtin modules are compiled into the current program and have no
            // manifest dependencies of their own, so they resolve directly.
            if builtin_version(name).is_some() {
                visiting.pop();
                visited.insert(name.to_string());
                resolved.push(name.to_string());
                return Ok(());
            }

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
                } else if let Some(version) = builtin_version(&dep.name) {
                    let available = semver::Version::parse(version)
                        .expect("builtin module versions must be valid semver");
                    if !requirement.matches(&available) {
                        return Err(format!(
                            "依赖 {} 需要 {}，当前内置版本为 {}",
                            dep.name, dep.requirement, version
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
    ///
    /// Required L1 plugins are implicit and always enabled, so enabling one is
    /// a no-op success.
    pub fn enable(&mut self, name: &str) -> Result<(), String> {
        if REQUIRED_PLUGIN_NAMES.contains(&name) {
            return Ok(());
        }

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
    ///
    /// Required L1 plugins cannot be disabled.
    pub fn disable(&mut self, name: &str) -> Result<(), String> {
        if REQUIRED_PLUGIN_NAMES.contains(&name) {
            return Err(format!("required plugin '{name}' cannot be disabled"));
        }

        let plugin = self
            .plugins
            .iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| format!("unknown plugin '{name}'"))?;
        plugin.enabled = false;
        Ok(())
    }
}

/// Best-effort match of a manager error key (`(plugin name or manifest path,
/// error)`) against a required plugin name.
fn error_key_matches(key: &str, required_name: &str) -> bool {
    if key == required_name {
        return true;
    }

    let slash_needle = format!("/{required_name}/");
    let backslash_needle = format!("\\{required_name}\\");
    key.contains(slash_needle.as_str()) || key.contains(backslash_needle.as_str())
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

fn default_layer(kind: PluginKind) -> PluginLayer {
    match kind {
        PluginKind::Ui | PluginKind::ChipDb => PluginLayer::Required,
        PluginKind::Adapter => PluginLayer::Cold,
        PluginKind::Protocol => PluginLayer::Hot,
    }
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

    fn write_builtin_manifest(root: &Path, plugin_dir: &str, content: &str) {
        let dir = root.join("plugins").join("builtin").join(plugin_dir);
        fs::create_dir_all(&dir).expect("create test builtin plugin dir");
        fs::write(dir.join("unipkg.toml"), content).expect("write test builtin manifest");
    }

    fn builtin_manifest(name: &str, kind: &str) -> String {
        format!(
            r#"
[package]
name = "{name}"
version = "1.0.0"
plugin_api = 1
kind = "{kind}"
layer = "required"
entry = "builtin"
provider = "builtin"

[dependencies]

[permissions]

[capabilities]
"#
        )
    }

    fn write_required_builtin_set(root: &Path) {
        for (name, kind) in [
            ("uni.tauri", "ui"),
            ("uni.hal", "adapter"),
            ("uni.chipdb", "chipdb"),
            ("uni.tauri.hexview", "ui"),
            ("uni.proto", "protocol"),
        ] {
            write_builtin_manifest(root, name, &builtin_manifest(name, kind));
        }
    }

    fn loaded_plugin(name: &str, version: &str, dependencies: Vec<Dependency>) -> LoadedPlugin {
        LoadedPlugin {
            manifest: PluginManifest {
                name: name.to_string(),
                version: semver::Version::parse(version).expect("valid test version"),
                plugin_api: PLUGIN_API_VERSION,
                kind: PluginKind::Adapter,
                layer: PluginLayer::Cold,
                entry: "plugin.exe".to_string(),
                dependencies,
                permissions: PluginPermissions::default(),
                capabilities: CapabilitySet::default(),
                os: Vec::new(),
                arch: Vec::new(),
                app: None,
                provider: None,
            },
            path: PathBuf::from(name),
            enabled: false,
            builtin: false,
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
provider = "vnd.example"
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
        assert_eq!(manifest.layer, PluginLayer::Cold);
        assert_eq!(manifest.entry, "plugin.exe");
        assert_eq!(manifest.provider.as_deref(), Some("vnd.example"));
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
    fn unipkg_toml_is_preferred_over_manifest_toml() {
        let root = test_root("unipkg-pref");
        let plugin_dir = root.join("plugins").join("demo");
        fs::create_dir_all(&plugin_dir).expect("create test plugin dir");
        fs::write(
            plugin_dir.join("unipkg.toml"),
            r#"
[package]
name = "vnd.example.unipkg"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"
"#,
        )
        .expect("write unipkg manifest");
        fs::write(
            plugin_dir.join("manifest.toml"),
            r#"
[package]
name = "vnd.example.legacy"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"
"#,
        )
        .expect("write legacy manifest");

        let candidates = manifest_candidates(&plugin_dir);
        assert_eq!(
            candidates,
            vec![
                plugin_dir.join("unipkg.toml"),
                plugin_dir.join("manifest.toml")
            ]
        );

        let manager = PluginManager::load(&root);
        assert_eq!(manager.errors, Vec::<(String, String)>::new());
        assert_eq!(manager.plugins.len(), 1);
        assert_eq!(manager.plugins[0].manifest.name, "vnd.example.unipkg");
        assert_eq!(manager.plugins[0].path, plugin_dir);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn layer_parses_and_defaults_by_kind() {
        let manifest = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.explicit"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
layer = "hot"
entry = "plugin.exe"
"#,
        )
        .expect("explicit layer should parse");
        assert_eq!(manifest.layer, PluginLayer::Hot);

        let ui = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.ui"
version = "0.1.0"
plugin_api = 1
kind = "ui"
entry = "plugin.exe"
"#,
        )
        .expect("ui manifest should parse");
        assert_eq!(ui.layer, PluginLayer::Required);

        let chipdb = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.chipdb"
version = "0.1.0"
plugin_api = 1
kind = "chipdb"
entry = "plugin.exe"
"#,
        )
        .expect("chipdb manifest should parse");
        assert_eq!(chipdb.layer, PluginLayer::Required);

        let adapter = PluginManifest::parse(MINIMAL).expect("adapter manifest should parse");
        assert_eq!(adapter.layer, PluginLayer::Cold);

        let protocol = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.protocol"
version = "0.1.0"
plugin_api = 1
kind = "protocol"
entry = "plugin.exe"
"#,
        )
        .expect("protocol manifest should parse");
        assert_eq!(protocol.layer, PluginLayer::Hot);

        let err = PluginManifest::parse(
            r#"
[package]
name = "vnd.example.bad-layer"
version = "0.1.0"
plugin_api = 1
kind = "adapter"
layer = "warm"
entry = "plugin.exe"
"#,
        )
        .expect_err("invalid layer must fail");
        assert!(err.contains("unknown plugin layer 'warm'"), "{err}");
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
    fn every_builtin_module_resolves_and_reports_its_version() {
        let manager = PluginManager {
            plugins: Vec::new(),
            errors: Vec::new(),
        };

        let modules = builtin_modules();
        assert_eq!(modules.len(), 13);
        for module in &modules {
            let resolved = manager
                .resolve_dependencies(&module.name)
                .unwrap_or_else(|e| panic!("builtin {} should resolve: {e}", module.name));
            assert_eq!(resolved, vec![module.name.clone()]);
            assert_eq!(builtin_version(&module.name), Some(module.version.as_str()));
        }
    }

    #[test]
    fn builtin_dependency_versions_are_checked() {
        let manager = PluginManager {
            plugins: vec![
                loaded_plugin(
                    "uses-hal-v1",
                    "1.0.0",
                    vec![Dependency {
                        name: "uni.hal".to_string(),
                        requirement: "^1".to_string(),
                    }],
                ),
                loaded_plugin(
                    "uses-hal-v2",
                    "1.0.0",
                    vec![Dependency {
                        name: "uni.hal".to_string(),
                        requirement: "^2".to_string(),
                    }],
                ),
            ],
            errors: Vec::new(),
        };

        let resolved = manager
            .resolve_dependencies("uses-hal-v1")
            .expect("uni.hal ^1 must resolve");
        assert_eq!(resolved, vec!["uses-hal-v1"]);

        let err = manager
            .resolve_dependencies("uses-hal-v2")
            .expect_err("uni.hal ^2 must fail");
        assert!(
            err.contains("依赖 uni.hal 需要 ^2，当前内置版本为 1.0.0"),
            "unexpected error: {err}"
        );
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
    fn load_scans_plugins_builtin_directory() {
        let root = test_root("builtin-scan");
        write_builtin_manifest(&root, "uni.hal", &builtin_manifest("uni.hal", "adapter"));

        let manager = PluginManager::load(&root);

        assert_eq!(manager.errors, Vec::<(String, String)>::new());
        assert_eq!(manager.plugins.len(), 1);
        assert_eq!(manager.plugins[0].manifest.name, "uni.hal");
        assert_eq!(manager.plugins[0].manifest.layer, PluginLayer::Required);
        assert_eq!(
            manager.plugins[0].manifest.provider.as_deref(),
            Some("builtin")
        );
        assert!(manager.plugins[0].builtin);
        assert!(manager.plugins[0].enabled);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn builtin_cold_plugins_start_enabled_but_can_be_disabled_in_state() {
        let root = test_root("builtin-cold");
        let manifest = r#"
[package]
name = "uni.hal.ch34x"
version = "1.0.0"
plugin_api = 1
kind = "adapter"
layer = "cold"
entry = "uni_ch34x_sidecar"
provider = "builtin"

[capabilities.spi]
enabled = true
pins = { cs = "CS0", sck = "SCK", mosi = "MOSI", miso = "MISO" }
max_frame = 4092
max_freq_khz = 60000
"#;
        write_builtin_manifest(&root, "uni.hal.ch34x", manifest);

        let manager = PluginManager::load(&root);
        assert_eq!(manager.plugins.len(), 1);
        assert!(manager.plugins[0].builtin);
        assert!(manager.plugins[0].enabled);

        save_plugin_state(&root, &manager).expect("state should save");
        let state = load_plugin_state(&root);
        assert_eq!(state.get("uni.hal.ch34x"), Some(&true));

        let mut disabled = manager;
        disabled
            .disable("uni.hal.ch34x")
            .expect("cold plugin can be disabled");
        save_plugin_state(&root, &disabled).expect("disabled state should save");
        let reloaded = PluginManager::load(&root);
        assert!(!reloaded.plugins[0].enabled);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn third_party_cold_plugins_stay_disabled_until_state_enables_them() {
        let root = test_root("cold-state");
        let manifest = r#"
[package]
name = "vnd.example.programmer"
version = "0.2.0"
plugin_api = 1
kind = "adapter"
layer = "cold"
entry = "plugin.exe"

[capabilities.spi]
enabled = true
pins = { cs = "CS0", sck = "SCK", mosi = "MOSI", miso = "MISO" }
max_frame = 4092
max_freq_khz = 60000
"#;
        write_manifest(&root, "vnd.example.programmer", manifest);
        let mut manager = PluginManager::load(&root);
        assert!(!manager.plugins[0].enabled);

        manager
            .enable("vnd.example.programmer")
            .expect("third-party cold plugin should enable");
        save_plugin_state(&root, &manager).expect("state should save");

        let reloaded = PluginManager::load(&root);
        assert!(reloaded.plugins[0].enabled);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn boot_check_ok_when_all_required_builtin_manifests_exist() {
        let root = test_root("boot-ok");
        write_required_builtin_set(&root);

        let manager = PluginManager::load(&root);
        assert_eq!(manager.plugins.len(), REQUIRED_PLUGIN_NAMES.len());

        let boot = manager.boot_check();
        assert!(
            boot.missing.is_empty(),
            "unexpected missing: {:?}",
            boot.missing
        );
        assert!(
            boot.invalid.is_empty(),
            "unexpected invalid: {:?}",
            boot.invalid
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn boot_check_reports_missing_required_plugins() {
        let manager = PluginManager {
            plugins: vec![loaded_plugin("uni.hal", "1.0.0", Vec::new())],
            errors: Vec::new(),
        };

        let boot = manager.boot_check();
        assert_eq!(
            boot.missing,
            vec![
                "uni.tauri".to_string(),
                "uni.chipdb".to_string(),
                "uni.tauri.hexview".to_string(),
                "uni.proto".to_string(),
            ]
        );
        assert!(boot.invalid.is_empty());
    }

    #[test]
    fn boot_check_reports_duplicate_required_plugin_as_invalid() {
        let root = test_root("boot-invalid");
        write_manifest(&root, "uni.hal", &builtin_manifest("uni.hal", "adapter"));
        write_builtin_manifest(&root, "uni.hal", &builtin_manifest("uni.hal", "adapter"));

        let manager = PluginManager::load(&root);
        assert_eq!(manager.plugins.len(), 1);
        assert_eq!(manager.errors.len(), 1);

        let boot = manager.boot_check();
        assert_eq!(boot.invalid, vec!["uni.hal".to_string()]);
        assert!(!boot.missing.contains(&"uni.hal".to_string()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn boot_check_reports_parse_error_required_plugin_as_invalid() {
        let manager = PluginManager {
            plugins: Vec::new(),
            errors: vec![(
                "/plugins/builtin/uni.hal/unipkg.toml".to_string(),
                "invalid TOML: expected a table".to_string(),
            )],
        };

        let boot = manager.boot_check();
        assert_eq!(boot.invalid, vec!["uni.hal".to_string()]);
        assert!(!boot.missing.contains(&"uni.hal".to_string()));
    }

    #[test]
    fn required_plugin_cannot_be_disabled_and_enable_is_noop() {
        let mut manager = PluginManager {
            plugins: vec![loaded_plugin("uni.hal", "1.0.0", Vec::new())],
            errors: Vec::new(),
        };

        let err = manager
            .disable("uni.hal")
            .expect_err("required plugin must not be disabled");
        assert!(
            err.contains("required plugin 'uni.hal' cannot be disabled"),
            "unexpected error: {err}"
        );
        assert!(!manager.plugins[0].enabled);

        manager
            .enable("uni.hal")
            .expect("enabling a required plugin is a no-op success");
        assert!(!manager.plugins[0].enabled);
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
