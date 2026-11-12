//! 开机自启：写当前用户的 `Run` 注册表项（不需要管理员权限）。
//!
//! 与 Python 的 `desktop/autostart.py` 同名同值：`HKCU\...\Run\AutoSignBJMF`。

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "AutoSignBJMF";

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 写进注册表的启动命令：带引号的完整 exe 路径。
pub fn launch_command() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("AutoSignBJMF.exe"));
    format!("\"{}\"", exe.display())
}

/// 开启 / 关闭开机自启；返回是否成功。
pub fn set_enabled(enabled: bool) -> bool {
    let subkey = wide(RUN_KEY);
    let value_name = wide(VALUE_NAME);
    unsafe {
        let mut key = std::mem::zeroed();
        let mut disposition = 0u32;
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE | KEY_READ,
            std::ptr::null(),
            &mut key,
            &mut disposition,
        );
        if status != 0 {
            return false;
        }
        let ok = if enabled {
            let command = wide(&launch_command());
            let bytes = std::slice::from_raw_parts(
                command.as_ptr() as *const u8,
                command.len() * std::mem::size_of::<u16>(),
            );
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                bytes.as_ptr(),
                bytes.len() as u32,
            ) == 0
        } else {
            // 不存在视为已经关闭
            RegDeleteValueW(key, value_name.as_ptr()) == 0 || true
        };
        RegCloseKey(key);
        ok
    }
}

/// 当前是否已开启。
pub fn is_enabled() -> bool {
    let subkey = wide(RUN_KEY);
    let value_name = wide(VALUE_NAME);
    unsafe {
        let mut key = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return false;
        }
        let mut size: u32 = 0;
        let status = RegQueryValueExW(
            key,
            value_name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        );
        RegCloseKey(key);
        status == 0 && size > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 启动命令带引号且指向_exe() {
        let command = launch_command();
        assert!(command.starts_with('"') && command.ends_with('"'));
        assert!(command.to_lowercase().contains(".exe"));
    }

    #[test]
    fn 值名与_python_版一致() {
        assert_eq!(VALUE_NAME, "AutoSignBJMF");
        assert_eq!(RUN_KEY, r"Software\Microsoft\Windows\CurrentVersion\Run");
    }

    #[test]
    fn 开关可以往返() {
        let before = is_enabled();
        // 打开
        assert!(set_enabled(true));
        assert!(is_enabled());
        // 恢复原状，避免污染开发机
        let _ = set_enabled(before);
        assert_eq!(is_enabled(), before);
    }
}
