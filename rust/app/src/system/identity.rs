//! 应用在系统里的「身份」：显示名 + AppUserModelID。
//!
//! 任务栏右键菜单顶上那行字、跳转列表标题、任务管理器「应用」一栏，来源有三处：
//!
//! 1. 窗口标题（`SetWindowTextW`）；
//! 2. 显式 AppUserModelID 注册的 `DisplayName`
//!    （`HKCU\Software\Classes\AppUserModelId\<AUMID>`）；
//! 3. exe 的 VERSIONINFO（`FileDescription`）。
//!
//! 只有 1、2 是我们能控制的，所以这里统一给：注册 AUMID 与它的显示名，
//! 并且**所有写进系统的字符串都走 UTF-16**（`wide()`），不碰 ANSI 路径 ——
//! 中文一旦被按系统 ANSI 代码页（中文机器是 GBK）解一次，就会变成
//! 「鐝骇榄旀柟鑷姩绛惧埌」这种乱码。
//!
//! 顺带一个好处：窗口现在是「关到托盘即销毁、点托盘再重建」，
//! 显式 AUMID 能让任务栏把重建出来的窗口认成同一个应用（按钮位置不乱跳）。

use std::ffi::c_void;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE,
    REG_SZ,
};
use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

/// 界面上、任务栏上、托盘里，统一用这一个名字。
pub const APP_NAME: &str = "班级魔方自动签到";

/// 显式 AppID：固定值，不要跟着版本改 —— 改了任务栏会把应用当成新的。
pub const AUMID: &str = "AutoSignBJMF.Desktop";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 写一个 REG_SZ（UTF-16，带结尾 0）。
fn set_string(key: *mut c_void, name: &str, value: &str) -> bool {
    let name_w = wide(name);
    let value_w = wide(value);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            value_w.as_ptr() as *const u8,
            value_w.len() * std::mem::size_of::<u16>(),
        )
    };
    unsafe {
        RegSetValueExW(
            key,
            name_w.as_ptr(),
            0,
            REG_SZ,
            bytes.as_ptr(),
            bytes.len() as u32,
        ) == ERROR_SUCCESS
    }
}

/// 把应用身份写进系统。**必须在创建第一个窗口之前调用**（AUMID 只对之后建的窗口生效）。
pub fn install() {
    // 1) 进程级 AppID
    let id = wide(AUMID);
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }

    // 2) HKCU\Software\Classes\AppUserModelId\<AUMID>
    //    壳层读这里的 DisplayName 作为任务栏按钮 / 跳转列表的标题。
    let subkey = wide(&format!(r"Software\Classes\AppUserModelId\{AUMID}"));
    unsafe {
        let mut key: *mut c_void = std::mem::zeroed();
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
        if status != ERROR_SUCCESS {
            crate::ui_debug(&format!("identity: 建注册表键失败 status={status}"));
            return;
        }
        let name_ok = set_string(key, "DisplayName", APP_NAME);
        // 图标也指向自己：任务栏按钮与 Alt+Tab 用得到
        let exe = std::env::current_exe().unwrap_or_default();
        let icon_ok = set_string(key, "IconUri", &exe.display().to_string());
        RegCloseKey(key);
        crate::ui_debug(&format!(
            "identity: AUMID={AUMID} DisplayName={name_ok} IconUri={icon_ok}"
        ));
    }
}

/// 读自己 exe 的 `FileDescription`（壳层在拿不到窗口标题时用它当名字）。
fn self_file_description() -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    let exe = std::env::current_exe().ok()?;
    let path: Vec<u16> = exe
        .display()
        .to_string()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, buffer.as_mut_ptr() as *mut _) == 0 {
            return None;
        }
        let sub: Vec<u16> = "\\StringFileInfo\\080404B0\\FileDescription\0"
            .encode_utf16()
            .collect();
        let mut value: *mut c_void = std::ptr::null_mut();
        let mut length = 0u32;
        if VerQueryValueW(
            buffer.as_ptr() as *const _,
            sub.as_ptr(),
            &mut value,
            &mut length,
        ) != 0
            && !value.is_null()
        {
            // 注意：VerQueryValueW 的 length 是**字符数**，不是字节数
            return Some(
                std::slice::from_raw_parts(value as *const u16, length as usize)
                    .iter()
                    .take_while(|c| **c != 0)
                    .map(|c| char::from_u32(*c as u32).unwrap_or('?'))
                    .collect(),
            );
        }
        None
    }
}

/// 启动自检：当前 exe 的版本信息是不是我们这个应用的名字。
///
/// 这一步是为了抓「构建工具不对」：GNU 的 `windres` 不认 `.rc` 里的
/// `#pragma code_page(65001)`，会把 UTF-8 字节当成 Unicode 写进资源，
/// 于是任务栏 / 任务管理器里显示成「鐝骇榄旀柟鑷姩绛惧埌」这种乱码。
/// 界面能改的只有窗口标题与 AUMID，资源里的名字只能靠换回 rc.exe 重编。
pub fn self_check() -> Option<String> {
    match self_file_description() {
        Some(desc) if desc == APP_NAME => None,
        Some(desc) => Some(format!(
            "当前 exe 的版本信息是「{desc}」，不是「{APP_NAME}」——多半是用 windres 编的旧构建，请换 rc.exe 重新打包"
        )),
        None => Some("当前 exe 没有版本信息（旧构建或资源没编进去）".to_string()),
    }
}

/// 把窗口标题按 UTF-16 显式写一遍。
///
/// gpui 内部用的是 `HSTRING`（也是宽字符），这里再补一次是为了两件事：
/// 窗口重建后壳层可能还留着旧值；以及万一哪天走了 ANSI 路径，
/// 中文会被按 GBK 解一次变成乱码 —— 这里统一兜住。
pub fn set_window_title(window: &gpui_kit::Window, title: &str) {
    let Some(hwnd) = crate::system::win::window_hwnd_of(window) else {
        return;
    };
    let text = wide(title);
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW(
            hwnd as windows_sys::Win32::Foundation::HWND,
            text.as_ptr(),
        );
    }
}
