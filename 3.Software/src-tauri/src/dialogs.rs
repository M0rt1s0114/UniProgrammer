//! Native file dialogs.
//!
//! On Windows this uses the real IFileDialog COM API (no plugin dependency).
//! Other platforms return an error for now; the frontend can fall back to the
//! browser-based `<input type=file>` / Blob download path.

#[cfg(target_os = "windows")]
mod imp {
    use std::ptr::null_mut;
    use windows::core::{IUnknown, HSTRING};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, FileSaveDialog, IFileDialog, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR,
        FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
    };

    /// Ensures CoUninitialize is called exactly once for a successful init.
    struct ComGuard(bool);

    impl ComGuard {
        fn new() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if hr.is_ok() {
                Ok(ComGuard(true))
            } else {
                // Tauri's main thread usually initialises COM already; if the
                // apartment is compatible we can continue without uninit.
                if hr.0 == 0x8001_0106u32 as i32 {
                    // RPC_E_CHANGED_MODE: a different apartment is active.
                    Err(format!("COM 初始化失败: {:#010X}", hr.0 as u32))
                } else {
                    Err(format!("COM 初始化失败: {:#010X}", hr.0 as u32))
                }
            }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn item_path(dialog: &IFileDialog) -> Result<String, String> {
        let item = unsafe { dialog.GetResult() }.map_err(|e| format!("获取对话框选择失败: {e}"))?;
        let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|e| format!("获取文件路径失败: {e}"))?;
        let result = unsafe { name.to_string() }.map_err(|e| format!("路径解码失败: {e}"));
        unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
        result
    }

    pub fn open_file() -> Result<Option<String>, String> {
        let _com = ComGuard::new()?;
        let dialog: IFileDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
                .map_err(|e| format!("创建打开对话框失败: {e}"))?;

        unsafe { dialog.SetOptions(FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR) }
            .map_err(|e| format!("配置打开对话框失败: {e}"))?;

        match unsafe { dialog.Show(HWND(null_mut())) } {
            Ok(()) => item_path(&dialog).map(Some),
            Err(_) => Ok(None), // user cancelled
        }
    }

    pub fn save_file(default_name: &str, default_ext: &str) -> Result<Option<String>, String> {
        let _com = ComGuard::new()?;
        let dialog: IFileDialog =
            unsafe { CoCreateInstance(&FileSaveDialog, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
                .map_err(|e| format!("创建保存对话框失败: {e}"))?;

        unsafe { dialog.SetOptions(FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT | FOS_NOCHANGEDIR) }
            .map_err(|e| format!("配置保存对话框失败: {e}"))?;
        unsafe { dialog.SetFileName(&HSTRING::from(default_name)) }
            .map_err(|e| format!("设置默认文件名失败: {e}"))?;
        unsafe { dialog.SetDefaultExtension(&HSTRING::from(default_ext)) }
            .map_err(|e| format!("设置默认扩展名失败: {e}"))?;

        match unsafe { dialog.Show(HWND(null_mut())) } {
            Ok(()) => item_path(&dialog).map(Some),
            Err(_) => Ok(None), // user cancelled
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn open_file() -> Result<Option<String>, String> {
        Err("原生文件对话框当前仅在 Windows 实现".into())
    }

    pub fn save_file(_default_name: &str, _default_ext: &str) -> Result<Option<String>, String> {
        Err("原生文件对话框当前仅在 Windows 实现".into())
    }
}

pub fn open_file() -> Result<Option<String>, String> {
    imp::open_file()
}

pub fn save_file(default_name: &str, default_ext: &str) -> Result<Option<String>, String> {
    imp::save_file(default_name, default_ext)
}
