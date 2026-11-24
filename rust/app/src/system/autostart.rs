//! 开机自启：写当前用户的 `Run` 注册表项（不需要管理员权限）。
//!
//! 与 Python 的 `desktop/autostart.py` 同名同值：`HKCU\...\Run\AutoSignBJMF`。
//!
//! # 便携模式为什么也写注册表
//!
//! 便携模式改变的只是**数据目录**（exe 同级有 `data/` 就就地读写），
//! 但「登录 Windows 后自动启动」这件事本身是**本机**的，Windows 只认两处：
//!
//! 1. `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`（当前用户，免管理员）
//! 2. 启动文件夹里的 `.lnk`（同样写在用户配置里，还要走 COM 建快捷方式）
//!
//! 两处都不是「跟着 U 盘走」的 —— U 盘拔掉，开机照样起不来。所以便携版
//! **默认不碰注册表**，只有用户在设置里主动打开「开机自动启动」时才写这一条。
//! 配置、Cookie、日志依旧只在 `data/` 里。
//!
//! 也正因为注册表是本机的、设置是跟着文件夹走的，两边会不一致（换机器、
//! 换盘符、程序被挪了位置）。启动时用 [`sync`] 按设置对齐一次，并且
//! [`is_active`] 以「这条命令是否指向**当前这份** exe」为准 —— 否则界面
//! 显示「已开启」、开机其实没起来。

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

/// 写进注册表的启动命令：带引号的完整 exe 路径 + `--tray`。
///
/// `--tray` 让开机自启走「托盘模式」：启动时不建窗口、不初始化 D3D，
/// 第一次点托盘才建窗。手动双击（没有这个参数）则照旧直接开窗。
pub fn launch_command() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("AutoSignBJMF.exe"));
    format!("\"{}\" --tray", exe.display())
}

/// 启动时对齐的四种结果（外加失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synced {
    /// 设置里开着，注册表里原来是空的或指向别处 —— 已按当前 exe 写好
    Written,
    /// 设置里开着，注册表里已经就是当前 exe（老版本的裸路径也算改写过）
    UpToDate,
    /// 设置里关着，删掉了属于本副本的那条
    Removed,
    /// 设置里关着，注册表里本来就没有
    Absent,
    /// 注册表里的那条属于别的副本，关的时候不替别人做决定
    Foreign,
    /// 写 / 删注册表失败（安全软件、组策略）
    Failed,
}

/// 把「设置里的意愿」落到本机注册表，返回做了什么。
///
/// 这是升级路径的兜底：
/// - 老版本写的是不带 `--tray` 的裸路径 → 改写；
/// - 换机器 / 换盘符后旧的绝对路径失效 → 改成当前 exe；
/// - 设置里关着但注册表里还有本副本的项（用户直接删了 json 之类）→ 删掉。
pub fn sync(enabled: bool) -> Synced {
    let stored = stored_command();
    if enabled {
        let want = launch_command();
        if stored.as_deref() == Some(want.as_str()) {
            return Synced::UpToDate;
        }
        if set_enabled(true) {
            Synced::Written
        } else {
            Synced::Failed
        }
    } else {
        match stored {
            None => Synced::Absent,
            Some(command) if command_is_ours(&command) => {
                if set_enabled(false) {
                    Synced::Removed
                } else {
                    Synced::Failed
                }
            }
            Some(_) => Synced::Foreign,
        }
    }
}

/// 读注册表里当前的启动命令（没有就是 `None`）。
pub fn stored_command() -> Option<String> {
    let subkey = wide(RUN_KEY);
    let value_name = wide(VALUE_NAME);
    unsafe {
        let mut key = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut size = (buffer.len() * std::mem::size_of::<u16>()) as u32;
        let status = RegQueryValueExW(
            key,
            value_name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut u8,
            &mut size,
        );
        RegCloseKey(key);
        if status != 0 {
            return None;
        }
        let chars = (size as usize / std::mem::size_of::<u16>()).min(buffer.len());
        let text: String = buffer[..chars]
            .iter()
            .take_while(|unit| **unit != 0)
            .filter_map(|unit| char::from_u32(*unit as u32))
            .collect();
        (!text.trim().is_empty()).then_some(text)
    }
}

/// 登录后会不会自动启动**这一份**程序。
///
/// 只看「注册表里有没有值」不够：值可能指向另一份拷贝（换了盘符、挪了位置），
/// 或者指向早就删掉的旧路径 —— 那和没开没区别。
pub fn is_active() -> bool {
    stored_command()
        .map(|command| command_is_ours(&command))
        .unwrap_or(false)
}

/// 从启动命令里取出 exe 路径：`"C:\A B\x.exe" --tray` → `C:\A B\x.exe`。
fn command_exe(command: &str) -> Option<&str> {
    let text = command.trim();
    if let Some(rest) = text.strip_prefix('"') {
        return rest.split('"').next().filter(|part| !part.is_empty());
    }
    text.split_whitespace().next()
}

/// 这条启动命令指向的是不是本进程的 exe。
fn command_is_ours(command: &str) -> bool {
    let Ok(ours) = std::env::current_exe() else {
        return false;
    };
    match command_exe(command) {
        Some(found) => same_file(std::path::Path::new(found), &ours),
        None => false,
    }
}

/// 路径比较：大小写不敏感，能解析就先解析（盘符、短路径、相对写法都能对上）。
fn same_file(left: &std::path::Path, right: &std::path::Path) -> bool {
    let normalize = |path: &std::path::Path| {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase()
    };
    normalize(left) == normalize(right)
}

/// 开启 / 关闭开机自启；返回是否成功。
pub fn set_enabled(enabled: bool) -> bool {
    if enabled {
        write_command(&launch_command())
    } else {
        delete_value()
    }
}

/// 按原样写一条启动命令（测试里用来还原开发机上的既有值）。
fn write_command(command: &str) -> bool {
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
        let command = wide(command);
        let bytes = std::slice::from_raw_parts(
            command.as_ptr() as *const u8,
            command.len() * std::mem::size_of::<u16>(),
        );
        let ok = RegSetValueExW(
            key,
            value_name.as_ptr(),
            0,
            REG_SZ,
            bytes.as_ptr(),
            bytes.len() as u32,
        ) == 0;
        RegCloseKey(key);
        ok
    }
}

/// 删掉启动项；本来就不存在也算成功。
fn delete_value() -> bool {
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
        let ok = RegDeleteValueW(key, value_name.as_ptr()) == 0;
        RegCloseKey(key);
        ok
    }
}

/// 注册表里有没有这一条（不关心它指向哪里）。测试与诊断用，
/// 界面判断「是否已开启」一律用 [`is_active`]。
#[cfg(test)]
pub fn is_enabled() -> bool {
    stored_command().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 启动命令带引号指向_exe且带托盘参数() {
        let command = launch_command();
        assert!(command.starts_with('"'));
        assert!(command.to_lowercase().contains(".exe"));
        // 开机自启走托盘模式：不建窗口、不初始化 D3D
        assert!(command.ends_with("\" --tray"), "实际是 {command}");
    }

    #[test]
    fn 值名与_python_版一致() {
        assert_eq!(VALUE_NAME, "AutoSignBJMF");
        assert_eq!(RUN_KEY, r"Software\Microsoft\Windows\CurrentVersion\Run");
    }

    /// 注册表是全局状态，两个用例同时改会互相踩；合成一个用例串行做。
    #[test]
    fn 注册表往返_含旧格式修正() {
        // 先记下开发机上原本那条（可能指向真正的 app），测完原样写回去
        let before = stored_command();

        // 1) 开关往返
        assert!(set_enabled(true));
        assert!(is_active(), "刚写进去的命令应当指向当前 exe");
        assert!(is_enabled());
        assert!(set_enabled(false));
        assert!(!is_active());

        // 2) 老版本写的是不带 --tray 的裸路径 → sync(true) 要改写成托盘模式
        let exe = std::env::current_exe().expect("测试进程自己的 exe");
        assert!(write_command(&format!("\"{}\"", exe.display())));
        assert!(is_active(), "裸路径也算指向本副本");
        assert_eq!(sync(true), Synced::Written);
        let command = stored_command().expect("刚写过，必须读得回来");
        assert!(command.ends_with("\" --tray"), "实际是 {command}");
        // 已经是对的再 sync 一次不应该重复写
        assert_eq!(sync(true), Synced::UpToDate);

        // 还原
        let restored = match before.as_deref() {
            Some(command) => write_command(command),
            None => set_enabled(false),
        };
        assert!(restored, "还原失败");
        assert_eq!(stored_command(), before);
    }

    #[test]
    fn 命令解析_带引号与不带引号() {
        assert_eq!(
            command_exe(r#""C:\Program Files\AutoSignBJMF\app.exe" --tray"#),
            Some(r"C:\Program Files\AutoSignBJMF\app.exe")
        );
        assert_eq!(command_exe(r"C:\app.exe --tray"), Some(r"C:\app.exe"));
        assert_eq!(command_exe("   "), None);
    }

    #[test]
    fn 指向别处的命令不算已开启() {
        // 换机器 / 换盘符之后，注册表里那条指向的是别的路径
        assert!(!command_is_ours(r#""C:\Other\AutoSignBJMF.exe" --tray"#));
        assert!(command_is_ours(&launch_command()));
    }
}
