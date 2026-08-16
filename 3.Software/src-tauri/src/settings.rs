use std::fs;
use std::path::PathBuf;

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
fn dir_is_writable(dir: &PathBuf) -> bool {
    let probe = dir.join(format!(".uniprogrammer-write-test-{}", std::process::id()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 选择设置文件位置：
/// - 已存在 exe 同级的 Setting.set：便携版模式，继续使用。
/// - 已存在用户主目录下的 Setting.set：安装版模式，继续使用。
/// - 都不存在：exe 目录可写则按便携版放 exe 同级，否则放 ~/UniProgrammer/Setting.set。
pub fn settings_file() -> PathBuf {
    let exe = exe_dir();
    let portable = exe.join(FILE_NAME);

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
