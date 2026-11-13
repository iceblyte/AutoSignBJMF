//! 窗口显隐：拿到本进程主窗口的 HWND，然后真正地隐藏 / 唤起。
//!
//! gpui 0.3 没有暴露 hide 接口，只能从平台层补：按进程 id 枚举顶层窗口，
//! 取面积最大的那个可见窗口当作主窗口。

use std::ffi::c_void;

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsZoomed, MessageBoxW, SetForegroundWindow, ShowWindow, ShowWindowAsync, GW_OWNER,
    MB_ICONINFORMATION, MB_OK, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWMAXIMIZED,
};

/// 转成 Windows 需要的以 0 结尾的 UTF-16。
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 按标题找到另一个进程的窗口并提到前台。
///
/// 单实例冲突时用：用户双击了第二个 exe，应该看到已经开着的那个窗口，
/// 而不是什么都没有（GUI 子系统下 eprintln 是看不见的）。
/// 隐藏到托盘的窗口也能被 `FindWindowW` 找到，这里会顺带 `SW_SHOW` 出来。
pub fn activate_window_by_title(title: &str) -> bool {
    let title = wide(title);
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let maximized = IsZoomed(hwnd) != 0;
        ShowWindowAsync(hwnd, if maximized { SW_SHOWMAXIMIZED } else { SW_SHOW });
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        true
    }
}

/// 弹一个系统消息框。GUI 子系统下没有控制台，这是唯一能确保用户看见的方式。
pub fn message_box(caption: &str, text: &str) {
    let text = wide(text);
    let caption = wide(caption);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

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
        if owner != search.pid {
            return 1;
        }
        // 注意：这里**不能**过滤 IsWindowVisible。
        // 最小化到托盘时窗口被 SW_HIDE，一旦跳过不可见窗口，
        // main_window() 就再也找不到句柄，hide 之后 show 不回来
        // （表现出来就是「托盘双击/菜单都打不开窗口」）。
        if GetWindow(hwnd, GW_OWNER) != std::ptr::null_mut() {
            return 1; // 有 owner 的是附属窗口，不是主窗口
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let area = (rect.right - rect.left) * (rect.bottom - rect.top);
        if area < 200 * 200 {
            return 1; // 消息窗口之类的尺寸很小
        }
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

    #[test]
    fn 找不到标题相同的窗口时返回_false() {
        // 造一个绝不会存在的标题，避免与真实窗口撞名
        assert!(!activate_window_by_title(
            "__AutoSignBJMF_不存在的窗口标题__"
        ));
    }
}
