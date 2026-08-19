use std::fs;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "Setting.set";

/// 可执行文件所在目录：
/// - 调试运行时是进程工作目录
/// - 发布运行时是 exe 所在目录（便携版资源同目录）
fn exe_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        std::env::current_dir().expect("无法获取工作目录")
    }
    #[cfg(not(debug_assertions))]
    {
        let exe = std::env::current_exe().expect("无法获取 exe 路径");
        exe.parent().unwrap().to_path_buf()
    }
}

/// 用户主目录下的 UniProgrammer 配置目录（所有平台统一为 ~/UniProgrammer）。
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn home_settings_file() -> Option<PathBuf> {
    home_dir().map(|home| home.join("UniProgrammer").join(FILE_NAME))
}

/// 检查目录是否可写：写入并删除一个探测文件。
fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".uniprogrammer-write-test-{}", std::process::id()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 识别常见的安装版目录：即使以管理员运行（目录可写），也按安装版规则
/// 使用 ~/UniProgrammer/Setting.set。
fn looks_installed(dir: &Path) -> bool {
    #[cfg(windows)]
    {
        let path = dir.to_string_lossy().to_lowercase();
        path.contains("\\program files") || path.contains("\\program files (x86)")
    }
    #[cfg(not(windows))]
    {
        let path = dir.to_string_lossy();
        path.starts_with("/usr/")
            || path.starts_with("/opt/")
            || path.starts_with("/snap/")
            || path.starts_with("/Applications/")
    }
}

/// 选择设置文件位置：
/// - 已存在 exe 同级的 Setting.set：便携版模式，继续使用。
/// - 已存在用户主目录下的 Setting.set：安装版模式，继续使用。
/// - 都不存在：exe 目录可写则按便携版放 exe 同级，否则放 ~/UniProgrammer/Setting.set。
pub fn settings_file() -> PathBuf {
    let exe = exe_dir();
    let portable = exe.join(FILE_NAME);

    // 安装版目录固定使用用户主目录，即使管理员运行或 exe 旁遗留旧文件
    if looks_installed(&exe) {
        return home_settings_file().unwrap_or(portable);
    }

    if portable.exists() {
        return portable;
    }

    if let Some(installed) = home_settings_file() {
        if installed.exists() {
            return installed;
        }
    }

    if dir_is_writable(&exe) {
        portable
    } else {
        home_settings_file().unwrap_or(portable)
    }
}

pub fn load() -> Result<String, String> {
    let path = settings_file();
    match fs::read_to_string(&path) {
        Ok(content) => Ok(content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(format!("读取设置文件失败 {}: {}", path.display(), err)),
    }
}

pub fn save(content: &str) -> Result<String, String> {
    let path = settings_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建设置目录失败 {}: {}", parent.display(), err))?;
    }
    fs::write(&path, content)
        .map_err(|err| format!("写入设置文件失败 {}: {}", path.display(), err))?;
    Ok(path.display().to_string())
}

/// 启动早期读取 `[general] debugConsole`，决定是否显示调试控制台。
/// 设置文件缺失或解析失败时按 false 处理（正式版不弹窗）。
pub fn startup_debug_console() -> bool {
    let path = settings_file();
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };

    let mut in_general = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.eq_ignore_ascii_case("[general]") {
            in_general = true;
            continue;
        }
        if line.starts_with('[') {
            in_general = false;
            continue;
        }
        if !in_general {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("debugConsole")
            && matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            )
        {
            return true;
        }
    }
    false
}
