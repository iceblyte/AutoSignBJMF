//! 窗口显隐：拿到本进程主窗口的 HWND，然后真正地隐藏 / 唤起。
//!
//! gpui 0.3 没有暴露 hide 接口，只能从平台层补：按进程 id 枚举顶层窗口，
//! 取面积最大的那个可见窗口当作主窗口。

use std::ffi::c_void;

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, IsZoomed,
    SetForegroundWindow, ShowWindow, ShowWindowAsync, SW_HIDE, SW_RESTORE, SW_SHOW,
    SW_SHOWMAXIMIZED,
};

/// 窗口矩形。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowBounds {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// 枚举时携带的参数：进程 id 进、最佳窗口出。
struct Search {
    pid: u32,
    best: Option<(i32, *mut c_void)>,
}

unsafe extern "system" fn visit(hwnd: *mut c_void, lparam: isize) -> i32 {
    unsafe {
        let search = &mut *(lparam as *mut Search);
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner != search.pid || IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let area = (rect.right - rect.left) * (rect.bottom - rect.top);
        if search.best.map(|(best, _)| area > best).unwrap_or(true) {
            search.best = Some((area, hwnd));
        }
    }
    1
}

/// 找到当前进程的主窗口句柄。
pub fn main_window() -> Option<*mut c_void> {
    let mut search = Search {
        pid: std::process::id(),
        best: None,
    };
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as isize);
    }
    search.best.map(|(_, hwnd)| hwnd)
}

/// 隐藏主窗口（等价于「最小化到托盘」）。
pub fn hide_main_window() -> bool {
    match main_window() {
        Some(hwnd) => unsafe { ShowWindow(hwnd, SW_HIDE) != 0 },
        None => false,
    }
}

/// 重新显示并把窗口提到前台；最大化状态会被保留。
pub fn show_main_window() -> bool {
    match main_window() {
        Some(hwnd) => unsafe {
            let maximized = IsZoomed(hwnd) != 0;
            ShowWindowAsync(hwnd, if maximized { SW_SHOWMAXIMIZED } else { SW_SHOW });
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
            true
        },
        None => false,
    }
}

/// 当前主窗口的矩形（截图与布局断言用）。
pub fn main_window_bounds() -> Option<WindowBounds> {
    let hwnd = main_window()?;
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    Some(WindowBounds {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 测试进程没有顶层窗口时安全返回() {
        assert!(main_window().is_none());
        assert!(main_window_bounds().is_none());
        assert!(!hide_main_window());
    }

    #[test]
    fn 矩形宽高计算() {
        let bounds = WindowBounds {
            left: 10,
            top: 20,
            right: 310,
            bottom: 220,
        };
        assert_eq!(bounds.width(), 300);
        assert_eq!(bounds.height(), 200);
    }
}
