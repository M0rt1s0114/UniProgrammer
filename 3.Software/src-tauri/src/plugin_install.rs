//! Offline plugin package installation.
//!
//! This module is intentionally Tauri-free. It installs a plugin from either
//! an unpacked folder or a `.unipkg` ZIP archive into
//! `<plugins_root>/plugins/<name>`. Both package formats use `unipkg.toml`
//! (preferred) or the legacy `manifest.toml` at the package root.

use crate::plugin::{manifest_candidates, PluginManifest};
use serde::Serialize;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Result of a successful plugin package install.
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub id: String,
    pub version: String,
    pub path: PathBuf,
}

/// Git hosting services for which raw manifest URLs can be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHost {
    GitHub,
    GitLab,
    Gitee,
}

/// Preview of a plugin git repository before installing it.
#[derive(Debug, Clone, Serialize)]
pub struct GitPluginPreview {
    pub url: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub layer: String,
    pub marker_file: String,
}

/// Recognize the git host in `url`, including `www.` host prefixes and
/// repository URLs that end in `.git`.
pub fn parse_git_host(url: &str) -> Option<GitHost> {
    let after = host_portion(url);
    known_host(after).map(|(host, _)| host)
}

/// Build the raw-file URL for `marker` at `HEAD` of the repository described
/// by `url`.
///
/// Supports `https://host/owner/repo[.git]` and `git@host:owner/repo.git`
/// forms.
pub fn raw_url_for(url: &str, marker: &str) -> Option<String> {
    let (host, owner, repo) = split_git_url(url)?;
    let raw = match host {
        GitHost::GitHub => {
            format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{marker}")
        }
        GitHost::GitLab => format!("https://gitlab.com/{owner}/{repo}/-/raw/HEAD/{marker}"),
        GitHost::Gitee => format!("https://gitee.com/{owner}/{repo}/raw/HEAD/{marker}"),
    };
    Some(raw)
}

/// Parse plugin manifest TOML using the shared manifest parser.
pub fn parse_plugin_toml(text: &str) -> Result<PluginManifest, String> {
    PluginManifest::parse(text)
}

/// Fetch a plugin manifest from a git repository for user confirmation.
///
/// For GitHub/GitLab/Gitee URLs the raw manifest files are fetched directly
/// over HTTPS. If that fails (or the URL is not one of those hosts, such as a
/// `file://` URL used in tests), a temporary inspection-only clone is created
/// before any user confirmation; the clone is removed immediately afterwards.
pub fn preview_git_repo(url: &str) -> Result<GitPluginPreview, String> {
    let markers = ["unipkg.toml", "manifest.toml"];
    let mut first_fetch_error: Option<String> = None;

    for marker in markers {
        let Some(raw_url) = raw_url_for(url, marker) else {
            continue;
        };
        match fetch_plugin_toml(&raw_url) {
            Ok(text) => {
                let manifest = parse_plugin_toml(&text)
                    .map_err(|e| format!("invalid plugin manifest at {raw_url}: {e}"))?;
                return Ok(preview_from_manifest(url, marker, manifest));
            }
            Err(e) => {
                if first_fetch_error.is_none() {
                    first_fetch_error = Some(e);
                }
            }
        }
    }

    // Fallback: temporary, inspection-only clone. This runs before the user
    // confirms the install; it is removed before the function returns.
    let temp_dir = std::env::temp_dir().join(format!("uniprog-preview-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    if let Err(clone_err) = run_git_clone(url, &temp_dir, None) {
        let raw_reason = first_fetch_error
            .unwrap_or_else(|| "URL does not map to a GitHub/GitLab/Gitee raw URL".to_string());
        return Err(format!(
            "failed to fetch plugin manifest from remote ({raw_reason}); fallback git clone also failed: {clone_err}"
        ));
    }

    let result = read_manifest_from_dir(&temp_dir).map(|(marker_path, manifest)| {
        let marker_file = marker_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unipkg.toml")
            .to_string();
        preview_from_manifest(url, &marker_file, manifest)
    });
    let _ = fs::remove_dir_all(&temp_dir);
    result
}

/// Clone a git repository into a staging directory and install it as a plugin
/// package.
pub fn install_git_repo(
    url: &str,
    plugins_root: &Path,
    replace: bool,
    branch: Option<&str>,
) -> Result<InstallResult, String> {
    fs::create_dir_all(plugins_root)
        .map_err(|e| io_error("create plugins root directory", plugins_root, e))?;

    let staging = plugins_root.join(format!(".git-staging-{}", std::process::id()));
    clean_path(&staging)?;

    if let Err(e) = run_git_clone(url, &staging, branch) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("failed to clone git repository {url}: {e}"));
    }

    let result = read_manifest_from_dir(&staging).and_then(|(_marker_path, _manifest)| {
        // The plugin package itself must not carry the inspection clone's VCS
        // metadata into `<plugins_root>/plugins/<id>`.
        let _ = fs::remove_dir_all(staging.join(".git"));
        install_folder(&staging, plugins_root, replace)
    });

    let _ = fs::remove_dir_all(&staging);
    result
}

fn preview_from_manifest(
    url: &str,
    marker_file: &str,
    manifest: PluginManifest,
) -> GitPluginPreview {
    GitPluginPreview {
        url: url.to_string(),
        name: manifest.name,
        version: manifest.version.to_string(),
        kind: manifest.kind.to_string(),
        layer: manifest.layer.to_string(),
        marker_file: marker_file.to_string(),
    }
}

fn fetch_plugin_toml(raw_url: &str) -> Result<String, String> {
    let response = ureq::get(raw_url)
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| format!("request failed for {raw_url}: {e}"))?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("request for {raw_url} returned HTTP {status}"));
    }
    response
        .into_string()
        .map_err(|e| format!("failed to read response body from {raw_url}: {e}"))
}

fn run_git_clone(url: &str, dest: &Path, branch: Option<&str>) -> Result<(), String> {
    let mut command = std::process::Command::new("git");
    command
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("--no-tags");
    if let Some(branch) = branch {
        command.arg("--branch").arg(branch);
    }
    command.arg(url).arg(dest).env("GIT_TERMINAL_PROMPT", "0");

    let output = command
        .output()
        .map_err(|e| format!("failed to run git clone: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git clone failed: {}", stderr.trim()));
    }
    Ok(())
}

fn host_portion(url: &str) -> &str {
    let url = url.trim();
    let after_scheme = match url.find("://") {
        Some(idx) => &url[idx + 3..],
        None => url,
    };
    let after_user = after_scheme
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(after_scheme);
    after_user.split(['?', '#']).next().unwrap_or(after_user)
}

fn known_host(after: &str) -> Option<(GitHost, usize)> {
    let lower = after.to_ascii_lowercase();
    const HOSTS: [(&str, GitHost); 6] = [
        ("www.github.com", GitHost::GitHub),
        ("github.com", GitHost::GitHub),
        ("www.gitlab.com", GitHost::GitLab),
        ("gitlab.com", GitHost::GitLab),
        ("www.gitee.com", GitHost::Gitee),
        ("gitee.com", GitHost::Gitee),
    ];

    for (prefix, host) in HOSTS {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest.is_empty() || rest.starts_with('/') || rest.starts_with(':') {
                return Some((host, prefix.len()));
            }
            return None;
        }
    }
    None
}

fn split_git_url(url: &str) -> Option<(GitHost, String, String)> {
    let after = host_portion(url);
    let (host, host_len) = known_host(after)?;
    let rest = &after[host_len..];
    let path = path_after_host(rest)?;
    let (owner, repo) = parse_owner_repo(path)?;
    Some((host, owner, repo))
}

fn path_after_host(rest: &str) -> Option<&str> {
    if let Some(path) = rest.strip_prefix('/') {
        return Some(path);
    }
    if let Some(path) = rest.strip_prefix(':') {
        // `git@host:owner/repo.git` scp-like form. An optional numeric port
        // (`host:443/owner/repo`) is accepted after the host name.
        if let Some((port, after_slash)) = path.split_once('/') {
            if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
                return Some(after_slash);
            }
        }
        return Some(path);
    }
    None
}

fn parse_owner_repo(path: &str) -> Option<(String, String)> {
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn clean_path(path: &Path) -> Result<(), String> {
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|e| io_error("clean path", path, e))?;
        } else {
            fs::remove_file(path).map_err(|e| io_error("clean path", path, e))?;
        }
    }
    Ok(())
}

/// Install an unpacked plugin folder.
///
/// `source` must be a directory containing `unipkg.toml` or `manifest.toml`.
/// The package is copied recursively into a staging directory under
/// `plugins_root` and then atomically renamed to
/// `<plugins_root>/plugins/<name>`.
pub fn install_folder(
    source: &Path,
    plugins_root: &Path,
    replace: bool,
) -> Result<InstallResult, String> {
    if !source.is_dir() {
        return Err(format!(
            "plugin package source is not a directory: {}",
            source.display()
        ));
    }

    let (_manifest_path, manifest) = read_manifest_from_dir(source)?;
    let name = manifest.name.clone();
    validate_plugin_name(&name)?;
    let version = manifest.version.to_string();

    let plugins_dir = plugins_root.join("plugins");
    fs::create_dir_all(&plugins_dir)
        .map_err(|e| io_error("create plugins directory", &plugins_dir, e))?;

    let staging = staging_dir(plugins_root, &name)?;
    fs::create_dir_all(&staging).map_err(|e| io_error("create staging directory", &staging, e))?;

    if let Err(e) = copy_dir_contents(source, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    if let Err(e) = validate_staged_manifest(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let dest = plugins_dir.join(&name);
    commit_staging(&staging, &dest, plugins_root, replace)?;

    Ok(InstallResult {
        id: name,
        version,
        path: dest,
    })
}

/// Install a `.unipkg` ZIP archive.
///
/// The archive must contain `unipkg.toml` or `manifest.toml` at its root
/// (leading `./` path components are ignored). All entries are extracted into
/// a staging directory with path-traversal protection; the staging directory
/// is then renamed to `<plugins_root>/plugins/<name>`.
pub fn install_unipkg(
    package_path: &Path,
    plugins_root: &Path,
    replace: bool,
) -> Result<InstallResult, String> {
    let file =
        fs::File::open(package_path).map_err(|e| io_error("open package file", package_path, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("failed to open ZIP package {}: {e}", package_path.display()))?;

    let mut manifest = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("failed to read ZIP entry {index}: {e}"))?;
        let entry_name = entry.name().to_string();
        if matches!(
            normalized_entry_name(&entry_name).as_str(),
            "unipkg.toml" | "manifest.toml"
        ) {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| format!("failed to read {} from ZIP package: {e}", entry_name))?;
            manifest = Some(PluginManifest::parse(&text).map_err(|e| {
                format!("invalid plugin manifest {} in ZIP package: {e}", entry_name)
            })?);
            break;
        }
    }

    let manifest = manifest.ok_or_else(|| {
        format!(
            "package {} does not contain unipkg.toml or manifest.toml at archive root",
            package_path.display()
        )
    })?;
    let name = manifest.name.clone();
    validate_plugin_name(&name)?;
    let version = manifest.version.to_string();

    let plugins_dir = plugins_root.join("plugins");
    fs::create_dir_all(&plugins_dir)
        .map_err(|e| io_error("create plugins directory", &plugins_dir, e))?;

    let staging = staging_dir(plugins_root, &name)?;
    fs::create_dir_all(&staging).map_err(|e| io_error("create staging directory", &staging, e))?;

    if let Err(e) = extract_archive(&mut archive, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    if let Err(e) = validate_staged_manifest(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let dest = plugins_dir.join(&name);
    commit_staging(&staging, &dest, plugins_root, replace)?;

    Ok(InstallResult {
        id: name,
        version,
        path: dest,
    })
}

fn read_manifest_from_dir(dir: &Path) -> Result<(PathBuf, PluginManifest), String> {
    let path = manifest_candidates(dir).into_iter().next().ok_or_else(|| {
        format!(
            "plugin package {} does not contain unipkg.toml or manifest.toml",
            dir.display()
        )
    })?;
    let text = fs::read_to_string(&path).map_err(|e| io_error("read manifest", &path, e))?;
    let manifest = PluginManifest::parse(&text)
        .map_err(|e| format!("invalid plugin manifest {}: {e}", path.display()))?;
    validate_plugin_name(&manifest.name)?;
    Ok((path, manifest))
}

fn validate_staged_manifest(dir: &Path) -> Result<PluginManifest, String> {
    let path = manifest_candidates(dir).into_iter().next().ok_or_else(|| {
        format!(
            "staged package {} does not contain unipkg.toml or manifest.toml",
            dir.display()
        )
    })?;
    let text = fs::read_to_string(&path).map_err(|e| io_error("read staged manifest", &path, e))?;
    PluginManifest::parse(&text)
        .map_err(|e| format!("invalid staged plugin manifest {}: {e}", path.display()))
}

fn validate_plugin_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("plugin manifest package.name is empty".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "invalid plugin name '{name}': path separators are not allowed"
        ));
    }
    let path = Path::new(name);
    if path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(format!("invalid plugin name '{name}'"));
    }
    Ok(())
}

fn staging_dir(plugins_root: &Path, name: &str) -> Result<PathBuf, String> {
    let staging = plugins_root.join(format!(".staging-{name}-{}", std::process::id()));
    if staging.exists() {
        if staging.is_dir() {
            fs::remove_dir_all(&staging)
                .map_err(|e| io_error("clean staging directory", &staging, e))?;
        } else {
            fs::remove_file(&staging).map_err(|e| io_error("clean staging path", &staging, e))?;
        }
    }
    Ok(staging)
}

fn copy_dir_contents(source: &Path, dest: &Path) -> Result<(), String> {
    let entries =
        fs::read_dir(source).map_err(|e| io_error("read package directory", source, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io_error("read package directory entry", source, e))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| io_error("inspect package entry", &from, e))?;
        if file_type.is_dir() {
            fs::create_dir_all(&to).map_err(|e| io_error("create package directory", &to, e))?;
            copy_dir_contents(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| io_error("copy package file", &from, e))?;
        }
    }
    Ok(())
}

fn extract_archive(archive: &mut zip::ZipArchive<fs::File>, staging: &Path) -> Result<(), String> {
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("failed to read ZIP entry {index}: {e}"))?;
        let entry_name = entry.name().to_string();
        let safe_path = safe_entry_path(&entry_name)?;
        let out_path = staging.join(safe_path);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| io_error("create package directory", &out_path, e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| io_error("create package directory", parent, e))?;
            }
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| io_error("create package file", &out_path, e))?;
            io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("failed to extract {}: {e}", entry_name))?;
        }
    }
    Ok(())
}

/// Normalize a ZIP entry name for root-manifest detection: backslashes become
/// `/` and leading `./` components are stripped.
fn normalized_entry_name(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    let mut stripped = normalized.as_str();
    while let Some(rest) = stripped.strip_prefix("./") {
        stripped = rest;
    }
    stripped.to_string()
}

/// Convert a ZIP entry name into a path safe to join onto the staging
/// directory. This is equivalent in spirit to `ZipFile::enclosed_name()` but
/// also normalizes backslash separators: absolute paths, Windows prefixes and
/// any `..` component are rejected.
fn safe_entry_path(name: &str) -> Result<PathBuf, String> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut safe = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "unsafe path in ZIP package: {name:?} (must stay inside package root)"
                ));
            }
        }
    }

    if safe.as_os_str().is_empty() {
        return Err(format!(
            "unsafe path in ZIP package: {name:?} (must stay inside package root)"
        ));
    }
    Ok(safe)
}

/// Move `staging` into `dest`, optionally replacing an existing destination.
///
/// With `replace` the existing destination is first moved to
/// `<plugins_root>/.old-<pid>`; if the staging rename fails the old package is
/// moved back on a best-effort basis.
fn commit_staging(
    staging: &Path,
    dest: &Path,
    plugins_root: &Path,
    replace: bool,
) -> Result<(), String> {
    if dest.exists() {
        if !replace {
            let _ = fs::remove_dir_all(staging);
            return Err(format!(
                "plugin destination already exists: {}",
                dest.display()
            ));
        }

        let backup = plugins_root.join(format!(".old-{}", std::process::id()));
        if backup.exists() {
            if backup.is_dir() {
                fs::remove_dir_all(&backup)
                    .map_err(|e| io_error("clean old backup directory", &backup, e))?;
            } else {
                fs::remove_file(&backup)
                    .map_err(|e| io_error("clean old backup path", &backup, e))?;
            }
        }

        fs::rename(dest, &backup).map_err(|e| {
            let _ = fs::remove_dir_all(staging);
            format!(
                "failed to move existing plugin to {}: {e}",
                backup.display()
            )
        })?;

        if let Err(e) = fs::rename(staging, dest) {
            let _ = fs::rename(&backup, dest);
            let _ = fs::remove_dir_all(staging);
            return Err(format!(
                "failed to move staged package into {}: {e}",
                dest.display()
            ));
        }
    } else {
        fs::rename(staging, dest).map_err(|e| {
            let _ = fs::remove_dir_all(staging);
            format!("failed to move staged package into {}: {e}", dest.display())
        })?;
    }

    Ok(())
}

fn io_error(action: &str, path: &Path, error: io::Error) -> String {
    format!("failed to {action} {}: {error}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
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
            "uniprogrammer-install-test-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_manifest(dir: &Path, content: &str, name: &str) {
        fs::create_dir_all(dir).expect("create manifest dir");
        fs::write(dir.join(name), content).expect("write manifest");
    }

    fn make_source(root: &Path, manifest_name: &str) -> PathBuf {
        let source = root.join("source");
        fs::create_dir_all(&source).expect("create source dir");
        write_manifest(&source, MINIMAL, manifest_name);
        fs::write(source.join("plugin.js"), "// test plugin").expect("write plugin payload");
        source
    }

    fn make_zip(root: &Path, zip_name: &str, entries: &[(&str, &str)]) -> PathBuf {
        let zip_path = root.join(zip_name);
        fs::create_dir_all(root).expect("create zip parent dir");
        let file = fs::File::create(&zip_path).expect("create zip file");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            writer
                .start_file(*name, options)
                .unwrap_or_else(|e| panic!("start zip entry {name}: {e}"));
            writer
                .write_all(content.as_bytes())
                .unwrap_or_else(|e| panic!("write zip entry {name}: {e}"));
        }
        writer.finish().expect("finish zip");
        zip_path
    }

    #[test]
    fn parse_git_host_recognizes_known_hosts() {
        assert_eq!(
            parse_git_host("https://github.com/owner/repo"),
            Some(GitHost::GitHub)
        );
        assert_eq!(
            parse_git_host("https://www.github.com/owner/repo.git"),
            Some(GitHost::GitHub)
        );
        assert_eq!(
            parse_git_host("https://gitlab.com/owner/repo"),
            Some(GitHost::GitLab)
        );
        assert_eq!(
            parse_git_host("https://www.gitlab.com/owner/repo.git"),
            Some(GitHost::GitLab)
        );
        assert_eq!(
            parse_git_host("https://gitee.com/owner/repo"),
            Some(GitHost::Gitee)
        );
        assert_eq!(
            parse_git_host("https://www.gitee.com/owner/repo.git"),
            Some(GitHost::Gitee)
        );
        assert_eq!(
            parse_git_host("git@github.com:owner/repo.git"),
            Some(GitHost::GitHub)
        );
        assert_eq!(
            parse_git_host("git@gitlab.com:owner/repo.git"),
            Some(GitHost::GitLab)
        );
        assert_eq!(
            parse_git_host("git@gitee.com:owner/repo.git"),
            Some(GitHost::Gitee)
        );
    }

    #[test]
    fn parse_git_host_rejects_unknown_hosts() {
        assert_eq!(parse_git_host("https://example.com/owner/repo"), None);
        assert_eq!(
            parse_git_host("https://github.com.evil.com/owner/repo"),
            None
        );
        assert_eq!(parse_git_host("not a url"), None);
    }

    #[test]
    fn raw_url_for_github_variants() {
        assert_eq!(
            raw_url_for("https://github.com/owner/repo", "unipkg.toml").as_deref(),
            Some("https://raw.githubusercontent.com/owner/repo/HEAD/unipkg.toml")
        );
        assert_eq!(
            raw_url_for("https://www.github.com/owner/repo.git", "manifest.toml").as_deref(),
            Some("https://raw.githubusercontent.com/owner/repo/HEAD/manifest.toml")
        );
        assert_eq!(
            raw_url_for("git@github.com:owner/repo.git", "unipkg.toml").as_deref(),
            Some("https://raw.githubusercontent.com/owner/repo/HEAD/unipkg.toml")
        );
    }

    #[test]
    fn raw_url_for_gitlab_variants() {
        assert_eq!(
            raw_url_for("https://gitlab.com/owner/repo", "unipkg.toml").as_deref(),
            Some("https://gitlab.com/owner/repo/-/raw/HEAD/unipkg.toml")
        );
        assert_eq!(
            raw_url_for("https://www.gitlab.com/owner/repo.git", "manifest.toml").as_deref(),
            Some("https://gitlab.com/owner/repo/-/raw/HEAD/manifest.toml")
        );
        assert_eq!(
            raw_url_for("git@gitlab.com:owner/repo.git", "unipkg.toml").as_deref(),
            Some("https://gitlab.com/owner/repo/-/raw/HEAD/unipkg.toml")
        );
    }

    #[test]
    fn raw_url_for_gitee_variants() {
        assert_eq!(
            raw_url_for("https://gitee.com/owner/repo", "unipkg.toml").as_deref(),
            Some("https://gitee.com/owner/repo/raw/HEAD/unipkg.toml")
        );
        assert_eq!(
            raw_url_for("https://www.gitee.com/owner/repo.git", "manifest.toml").as_deref(),
            Some("https://gitee.com/owner/repo/raw/HEAD/manifest.toml")
        );
        assert_eq!(
            raw_url_for("git@gitee.com:owner/repo.git", "unipkg.toml").as_deref(),
            Some("https://gitee.com/owner/repo/raw/HEAD/unipkg.toml")
        );
    }

    #[test]
    fn raw_url_for_malformed_returns_none() {
        assert_eq!(
            raw_url_for("https://example.com/owner/repo", "unipkg.toml"),
            None
        );
        assert_eq!(raw_url_for("https://github.com/owner", "unipkg.toml"), None);
        assert_eq!(
            raw_url_for("https://github.com/owner/repo/sub", "unipkg.toml"),
            None
        );
        assert_eq!(
            raw_url_for("https://github.com.evil.com/owner/repo", "unipkg.toml"),
            None
        );
        assert_eq!(raw_url_for("not a url", "unipkg.toml"), None);
    }

    #[test]
    fn install_folder_copies_contents() {
        let root = test_root("folder");
        let plugins_root = root.join("plugins-root");
        let source = make_source(&root, "unipkg.toml");

        let result = install_folder(&source, &plugins_root, false).expect("install folder");

        assert_eq!(result.id, "vnd.example.minimal");
        assert_eq!(result.version, "0.1.0");
        let dest = plugins_root.join("plugins").join("vnd.example.minimal");
        assert_eq!(result.path, dest);
        assert!(dest.join("unipkg.toml").is_file());
        assert!(dest.join("plugin.js").is_file());
        assert_eq!(
            fs::read_to_string(dest.join("plugin.js")).expect("read copied payload"),
            "// test plugin"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_folder_accepts_legacy_manifest() {
        let root = test_root("folder-legacy");
        let plugins_root = root.join("plugins-root");
        let source = make_source(&root, "manifest.toml");

        let result = install_folder(&source, &plugins_root, false).expect("install folder");

        assert_eq!(result.id, "vnd.example.minimal");
        assert!(result.path.join("manifest.toml").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_zip_extracts_package() {
        let root = test_root("zip");
        let plugins_root = root.join("plugins-root");
        let zip_path = make_zip(
            &root,
            "package.zip",
            &[
                ("unipkg.toml", MINIMAL),
                ("plugin.js", "// zipped plugin"),
                ("assets/data.json", "{}"),
            ],
        );

        let result = install_unipkg(&zip_path, &plugins_root, false).expect("install zip");

        assert_eq!(result.id, "vnd.example.minimal");
        assert_eq!(result.version, "0.1.0");
        let dest = plugins_root.join("plugins").join("vnd.example.minimal");
        assert_eq!(result.path, dest);
        assert!(dest.join("unipkg.toml").is_file());
        assert!(dest.join("plugin.js").is_file());
        assert!(dest.join("assets").join("data.json").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn replace_existing_moves_old_to_backup() {
        let root = test_root("replace");
        let plugins_root = root.join("plugins-root");

        let source_v1 = root.join("source-v1");
        fs::create_dir_all(&source_v1).expect("create v1 source");
        write_manifest(
            &source_v1,
            r#"
[package]
name = "vnd.example.replaceable"
version = "1.0.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"
"#,
            "unipkg.toml",
        );

        let source_v2 = root.join("source-v2");
        fs::create_dir_all(&source_v2).expect("create v2 source");
        write_manifest(
            &source_v2,
            r#"
[package]
name = "vnd.example.replaceable"
version = "2.0.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.exe"
"#,
            "unipkg.toml",
        );

        let first = install_folder(&source_v1, &plugins_root, false).expect("install v1");
        assert_eq!(first.version, "1.0.0");

        let second = install_folder(&source_v2, &plugins_root, true).expect("replace with v2");
        assert_eq!(second.version, "2.0.0");

        let dest = plugins_root.join("plugins").join("vnd.example.replaceable");
        assert!(dest.is_dir());
        let installed_manifest =
            fs::read_to_string(dest.join("unipkg.toml")).expect("read installed manifest");
        assert!(installed_manifest.contains("2.0.0"));

        let backup = plugins_root.join(format!(".old-{}", std::process::id()));
        assert!(backup.is_dir(), "old package should be backed up");
        let backup_manifest =
            fs::read_to_string(backup.join("unipkg.toml")).expect("read backup manifest");
        assert!(backup_manifest.contains("1.0.0"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn replace_false_errors_when_destination_exists() {
        let root = test_root("exists");
        let plugins_root = root.join("plugins-root");
        let source = make_source(&root, "unipkg.toml");

        install_folder(&source, &plugins_root, false).expect("first install");
        let err = install_folder(&source, &plugins_root, false).expect_err("duplicate must fail");

        assert!(err.contains("already exists"), "{err}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_missing_manifest_is_rejected() {
        let root = test_root("no-manifest");
        let plugins_root = root.join("plugins-root");
        let source = root.join("source");
        fs::create_dir_all(&source).expect("create source dir");
        fs::write(source.join("plugin.js"), "// no manifest").expect("write payload");

        let err =
            install_folder(&source, &plugins_root, false).expect_err("missing manifest must fail");
        assert!(
            err.contains("does not contain unipkg.toml or manifest.toml"),
            "{err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn zip_missing_root_manifest_is_rejected() {
        let root = test_root("zip-no-manifest");
        let plugins_root = root.join("plugins-root");
        let zip_path = make_zip(
            &root,
            "package.zip",
            &[("subdir/unipkg.toml", MINIMAL), ("plugin.js", "// zipped")],
        );

        let err = install_unipkg(&zip_path, &plugins_root, false)
            .expect_err("missing root manifest must fail");
        assert!(
            err.contains("does not contain unipkg.toml or manifest.toml at archive root"),
            "{err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn zip_path_traversal_is_rejected() {
        let root = test_root("zip-traversal");
        let plugins_root = root.join("plugins-root");
        let zip_path = make_zip(
            &root,
            "package.zip",
            &[("unipkg.toml", MINIMAL), ("../evil.txt", "evil")],
        );

        let err = install_unipkg(&zip_path, &plugins_root, false).expect_err("traversal must fail");
        assert!(err.contains("unsafe path in ZIP package"), "{err}");

        let _ = fs::remove_dir_all(&root);
    }

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn run_git_commit(repo: &Path) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "init"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("run git commit");
        assert!(status.success(), "git commit failed");
    }

    fn file_url(path: &Path) -> String {
        format!(
            "file:///{}",
            path.to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
        )
    }

    #[test]
    #[ignore = "requires local git"]
    fn preview_and_install_git_repo_from_local_file_url() {
        if !git_available() {
            eprintln!("git is not available; skipping");
            return;
        }

        let root = test_root("git-repo");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).expect("create git repo dir");
        fs::write(repo.join("unipkg.toml"), MINIMAL).expect("write unipkg manifest");
        fs::write(repo.join("plugin.js"), "// local git plugin").expect("write plugin payload");

        run_git(&repo, &["init"]);
        run_git(&repo, &["add", "-A"]);
        run_git_commit(&repo);

        let url = file_url(&repo);
        let preview = preview_git_repo(&url).expect("preview local git repo");
        assert_eq!(preview.name, "vnd.example.minimal");
        assert_eq!(preview.version, "0.1.0");
        assert_eq!(preview.kind, "adapter");
        assert_eq!(preview.layer, "cold");
        assert_eq!(preview.marker_file, "unipkg.toml");

        let plugins_root = root.join("plugins-root");
        let installed =
            install_git_repo(&url, &plugins_root, false, None).expect("install local git repo");
        assert_eq!(installed.id, "vnd.example.minimal");
        assert_eq!(installed.version, "0.1.0");
        let dest = plugins_root.join("plugins").join("vnd.example.minimal");
        assert_eq!(installed.path, dest);
        assert!(dest.join("unipkg.toml").is_file());
        assert_eq!(
            fs::read_to_string(dest.join("plugin.js")).expect("read installed payload"),
            "// local git plugin"
        );
        assert!(
            !dest.join(".git").exists(),
            "clone metadata must not be installed"
        );
        assert!(
            !plugins_root
                .join(format!(".git-staging-{}", std::process::id()))
                .exists(),
            "clone staging must be removed"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
