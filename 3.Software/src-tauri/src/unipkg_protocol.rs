//! `unipkg://` custom protocol.
//!
//! UI plugins are plain static web packages. `unipkg://<plugin-name>/<path>`
//! serves files from the resolved plugin directory (installed plugins first,
//! then `plugins/builtin/`), which makes L1 UI packages — including the main
//! window shell (`uni.ui.webview`) and embedded contribution pages such as
//! `uni.hexview` — replaceable without touching the L0 executable.
//!
//! In debug builds the two built-in UI plugins are redirected to their Vite
//! dev servers (started by `npm run dev`) so hot reload keeps working.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use tauri::http::{header, Response, StatusCode};
use uni_plugin::{PluginKind, PluginManager};

/// Resolved web root for one UI plugin package.
#[derive(Debug, Clone)]
pub struct WebPluginAsset {
    pub name: String,
    pub package_dir: PathBuf,
    pub entry: String,
}

/// Static snapshot of every loadable UI plugin.
#[derive(Debug, Default)]
pub struct UnipkgProtocol {
    plugins: HashMap<String, WebPluginAsset>,
}

impl UnipkgProtocol {
    pub fn from_manager(manager: &PluginManager) -> Self {
        let mut plugins = HashMap::new();
        for loaded in &manager.plugins {
            if loaded.manifest.kind != PluginKind::Ui {
                continue;
            }
            let entry = if loaded.manifest.entry == "builtin" {
                "dist/index.html".to_string()
            } else {
                loaded.manifest.entry.clone()
            };
            plugins.insert(
                loaded.manifest.name.clone(),
                WebPluginAsset {
                    name: loaded.manifest.name.clone(),
                    package_dir: loaded.path.clone(),
                    entry,
                },
            );
        }
        Self { plugins }
    }

    pub fn register<R: tauri::Runtime + 'static>(
        builder: tauri::Builder<R>,
        protocol: Self,
    ) -> tauri::Builder<R> {
        builder.register_uri_scheme_protocol("unipkg", move |_ctx, request| {
            protocol.respond(request.uri())
        })
    }

    fn respond(&self, uri: &tauri::http::Uri) -> Response<Vec<u8>> {
        let host = uri.host().unwrap_or_default();
        let raw_path = uri.path();

        // Canonical form: `unipkg://localhost/<plugin>/<path>` (Windows/Android
        // WebViews translate it to `http(s)://unipkg.localhost/<plugin>/<path>`
        // before wry reverts it). The host-based legacy form
        // `unipkg://<plugin>/<path>` is still accepted for older packages.
        let (plugin_name, plugin_path) = if host == "localhost" {
            let path = raw_path.trim_start_matches('/');
            match path.split_once('/') {
                Some((plugin, rest)) => (plugin.to_string(), rest.to_string()),
                None => (path.to_string(), String::new()),
            }
        } else {
            (
                host.to_string(),
                raw_path.trim_start_matches('/').to_string(),
            )
        };

        #[cfg(debug_assertions)]
        if let Some(port) = dev_server_port(&plugin_name) {
            let path = if plugin_path.is_empty() {
                "/".to_string()
            } else {
                format!("/{plugin_path}")
            };
            return redirect_html(&format!("http://127.0.0.1:{port}{path}"));
        }

        let Some(asset) = self.plugins.get(&plugin_name) else {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("未找到插件：{plugin_name}\n\nPlugin not found: {plugin_name}"),
            );
        };

        let relative = if plugin_path.is_empty() {
            asset.entry.as_str()
        } else if safe_relative_path(&plugin_path) {
            plugin_path.as_str()
        } else {
            return error_response(
                StatusCode::FORBIDDEN,
                "路径越界，拒绝访问。\n\nPath traversal rejected.",
            );
        };

        let candidate = asset.package_dir.join(relative);
        let root_canonical = match asset.package_dir.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    &format!(
                        "插件目录不存在：{}\n\nPlugin directory missing.",
                        asset.package_dir.display()
                    ),
                );
            }
        };
        let file_canonical = match candidate.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    &format!("未找到资源：{relative}\n\nAsset not found: {relative}"),
                );
            }
        };
        if !file_canonical.starts_with(&root_canonical) || !file_canonical.is_file() {
            return error_response(
                StatusCode::FORBIDDEN,
                "资源路径越界，拒绝访问。\n\nAsset path rejected.",
            );
        }

        let Ok(bytes) = std::fs::read(&file_canonical) else {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("读取资源失败：{relative}\n\nFailed to read asset: {relative}"),
            );
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type(&file_canonical))
            .header(header::CACHE_CONTROL, "no-store")
            .body(bytes)
            .expect("static response is valid")
    }
}

fn safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

fn error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>unipkg {}</title></head>\
         <body style=\"background:#0d0f12;color:#e8edf5;font-family:monospace;padding:24px\">\
         <pre>{}</pre></body></html>",
        status.as_u16(),
        html_escape(message)
    );
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(body.into_bytes())
        .expect("static response is valid")
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(debug_assertions)]
fn dev_server_port(plugin_name: &str) -> Option<u16> {
    match plugin_name {
        "uni.ui.webview" => Some(1420),
        "uni.hexview" => Some(1421),
        _ => None,
    }
}

#[cfg(debug_assertions)]
fn redirect_html(target: &str) -> Response<Vec<u8>> {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"refresh\" content=\"0;url={target}\">\
         <script>location.replace('{target}')</script></head>\
         <body><a href=\"{target}\">redirecting to dev server…</a></body></html>",
        target = html_escape(target)
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(body.into_bytes())
        .expect("static response is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uni_plugin::PluginManager;

    fn test_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "uniprog-protocol-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolves_ui_plugin_entry_and_assets() {
        let root = test_root("entry");
        let plugin_dir = root.join("plugins").join("builtin").join("uni.hexview");
        let dist = plugin_dir.join("dist");
        let assets = dist.join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(dist.join("index.html"), "<html></html>").unwrap();
        fs::write(assets.join("app.js"), "console.log(1)").unwrap();
        fs::write(
            plugin_dir.join("unipkg.toml"),
            "[package]\nname = \"uni.hexview\"\nversion = \"1.0.0\"\nplugin_api = 1\n\
             kind = \"ui\"\nlayer = \"required\"\nentry = \"dist/index.html\"\n",
        )
        .unwrap();

        let manager = PluginManager::load(&root);
        let protocol = UnipkgProtocol::from_manager(&manager);
        let asset = protocol.plugins.get("uni.hexview").unwrap();
        assert_eq!(asset.entry, "dist/index.html");
        assert!(asset.package_dir.join("dist/index.html").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(safe_relative_path("dist/index.html"));
        assert!(!safe_relative_path("../chiplib.bin"));
        assert!(!safe_relative_path("dist/../../chiplib.bin"));
        assert!(!safe_relative_path("C:/windows/win.ini"));
    }

    #[test]
    fn serves_package_assets_and_rejects_missing_and_traversal() {
        let root = test_root("serve");
        let plugin_dir = root.join("plugins").join("builtin").join("vnd.test.ui");
        let dist = plugin_dir.join("dist");
        let assets = dist.join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(dist.join("index.html"), "<html>ok</html>").unwrap();
        fs::write(assets.join("app.js"), "console.log('plugin asset')").unwrap();
        fs::write(
            plugin_dir.join("unipkg.toml"),
            "[package]\nname = \"vnd.test.ui\"\nversion = \"1.0.0\"\nplugin_api = 1\n\
             kind = \"ui\"\nlayer = \"required\"\nentry = \"dist/index.html\"\n",
        )
        .unwrap();

        let manager = PluginManager::load(&root);
        let protocol = UnipkgProtocol::from_manager(&manager);

        let entry_uri: tauri::http::Uri = "unipkg://localhost/vnd.test.ui/".parse().unwrap();
        let entry = protocol.respond(&entry_uri);
        assert_eq!(entry.status(), StatusCode::OK);
        assert_eq!(entry.body(), b"<html>ok</html>");

        // The host-style legacy form must keep working.
        let legacy_uri: tauri::http::Uri = "unipkg://vnd.test.ui/".parse().unwrap();
        assert_eq!(protocol.respond(&legacy_uri).status(), StatusCode::OK);

        // Vite emits `./assets/...` references from `dist/index.html`; the
        // protocol must serve them from `<package>/dist/assets/...`.
        let asset_uri: tauri::http::Uri = "unipkg://localhost/vnd.test.ui/dist/assets/app.js"
            .parse()
            .unwrap();
        let asset = protocol.respond(&asset_uri);
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(asset.body(), b"console.log('plugin asset')");

        let missing_uri: tauri::http::Uri =
            "unipkg://localhost/vnd.test.ui/missing.js".parse().unwrap();
        assert_eq!(
            protocol.respond(&missing_uri).status(),
            StatusCode::NOT_FOUND
        );

        let traversal_uri: tauri::http::Uri = "unipkg://localhost/vnd.test.ui/../unipkg.toml"
            .parse()
            .unwrap();
        assert_eq!(
            protocol.respond(&traversal_uri).status(),
            StatusCode::FORBIDDEN
        );

        let _ = fs::remove_dir_all(&root);
    }
}
