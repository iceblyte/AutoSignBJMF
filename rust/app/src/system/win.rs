//! 窗口显隐：拿到本进程主窗口的 HWND，然后真正地隐藏 / 唤起。
//!
//! gpui 0.3 没有暴露 hide 接口，只能从平台层补：按进程 id 枚举顶层窗口，
//! 取面积最大的那个可见窗口当作主窗口。

use std::ffi::c_void;

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetCursorPos, GetForegroundWindow, GetWindow, GetWindowRect,
    GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, IsZoomed, MessageBoxW, SetForegroundWindow, ShowWindow,
    ShowWindowAsync, GW_OWNER, MB_ICONINFORMATION, MB_OK, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNA,
    SW_SHOWMAXIMIZED,
};
use windows_sys::Win32::Foundation::POINT;

/// 转成 Windows 需要的以 0 结尾的 UTF-16。
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 把窗口抢到前台。
///
/// `SetForegroundWindow` 有个系统限制：调用方进程不是当前前台进程时，
/// 系统会**静默失败**（返回 0，窗口只闪一下任务栏）。
/// 「从托盘/主窗口点设置，设置窗口却留在后面」就是这么来的：
/// 窗口确实显示了，只是被主窗口盖着，看起来像没打开。
///
/// 常见做法是先 `AttachThreadInput` 到前台线程再设置，成功后再解绑；
/// 这里再加上 `BringWindowToTop` 与一次 topmost 往返兜底。
fn force_foreground(hwnd: *mut c_void) {
    use windows_sys::Win32::System::Threading::AttachThreadInput;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetWindowPos,
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    unsafe {
        let foreground = GetForegroundWindow();
        let mut foreground_pid = 0u32;
        let foreground_thread = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, &mut foreground_pid)
        };
        let target_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());

        let attached = foreground_thread != 0
            && foreground_thread != target_thread
            && AttachThreadInput(foreground_thread, target_thread, 1) != 0;

        SetForegroundWindow(hwnd);
        BringWindowToTop(hwnd);

        if attached {
            AttachThreadInput(foreground_thread, target_thread, 0);
        }

        // 仍然不是前台就做一次 topmost 往返：这一步能绕开前台锁，
        // 顺序保证窗口最终回到普通层级，不会一直压在别人上面。
        if GetForegroundWindow() != hwnd {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            SetForegroundWindow(hwnd);
        }
    }
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
        force_foreground(hwnd);
        true
    }
}

/// 桌面工作区（去掉任务栏的那块）的 (left, top, right, bottom)。
pub fn work_area() -> (i32, i32, i32, i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut RECT as *mut c_void, 0)
    };
    if ok == 0 {
        // 拿不到就退回主屏尺寸
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        return (0, 0, w, h);
    }
    (rect.left, rect.top, rect.right, rect.bottom)
}

/// 读剪贴板里的文本（CF_UNICODETEXT）。
///
/// 输入框需要支持 Ctrl+V —— gpui 的 key_down 只给到按键，不会自动把
/// 剪贴板内容塞进来，得自己读。
pub fn clipboard_text() -> Option<String> {
    use windows_sys::Win32::Foundation::HGLOBAL;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
            return None;
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT as u32);
        if handle.is_null() {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(handle as HGLOBAL) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        GlobalUnlock(handle as HGLOBAL);
        CloseClipboard();
        Some(String::from_utf16_lossy(slice))
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
        if owner != search.pid || IsWindowVisible(hwnd) == 0 {
            return 1;
        }
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

/// 从 gpui 的 `Window` 直接取 HWND。
///
/// 比 `EnumWindows` 可靠得多：窗口是被 Hide 还是 Show、有没有被最小化，
/// 都不影响这里拿到的是「就是这个窗口」的句柄，不会认错对象。
pub fn window_hwnd_of(window: &gpui_kit::Window) -> Option<*mut c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut c_void),
        _ => None,
    }
}

/// 隐藏指定窗口（主窗口「最小化到托盘」用）。
pub fn hide_window(window: &gpui_kit::Window) -> bool {
    hide_window_with(window, false)
}

/// 隐藏主窗口，并把它记进唤醒缓存。
///
/// 只有主窗口能写缓存 —— 设置窗口也走 hide 的话，缓存会被它覆盖，
/// 之后托盘「打开主窗口」就会把设置窗口叫出来。
pub fn hide_main_window_of(window: &gpui_kit::Window) -> bool {
    hide_window_with(window, true)
}

fn hide_window_with(window: &gpui_kit::Window, remember: bool) -> bool {
    let Some(hwnd) = window_hwnd_of(window) else {
        return false;
    };
    if remember {
        // 窗口一旦隐藏，EnumWindows 就找不到它了，唤回只能靠这个句柄
        if let Ok(mut guard) = cache().lock() {
            *guard = Some(hwnd as isize);
        }
    }
    unsafe { ShowWindow(hwnd, SW_HIDE) != 0 }
}

/// 把指定窗口显示出来，最小化的先还原。
pub fn show_window(window: &gpui_kit::Window) -> bool {
    let Some(hwnd) = window_hwnd_of(window) else {
        return false;
    };
    if let Ok(mut guard) = cache().lock() {
        *guard = Some(hwnd as isize);
    }
    unsafe {
        let command = if IsIconic(hwnd) != 0 {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        ShowWindow(hwnd, command);
        force_foreground(hwnd);
        true
    }
}

/// 这个窗口当前是否可见（隐藏 / 最小化都算不可见）。
pub fn window_is_visible(window: &gpui_kit::Window) -> bool {
    match window_hwnd_of(window) {
        Some(hwnd) => unsafe { IsWindowVisible(hwnd) != 0 },
        None => false,
    }
}

/// 让系统标题栏跟随明暗（否则深色内容配白色标题栏很割裂）。
pub fn set_dark_titlebar(window: &gpui_kit::Window, dark: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let Some(hwnd) = window_hwnd_of(window) else {
        return;
    };
    let value: i32 = if dark { 1 } else { 0 };
    let size = std::mem::size_of::<i32>() as u32;
    unsafe {
        // 20 是 Win10 20H1 之后的属性号；更早的版本是 19。两个都试，失败就退。
        let first = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &value as *const i32 as *const c_void,
            size,
        );
        if first != 0 {
            DwmSetWindowAttribute(hwnd, 19, &value as *const i32 as *const c_void, size);
        }
        // 切完让窗口重画一次标题栏。
        //
        // `SWP_NOACTIVATE` 必须带上：没有它 SetWindowPos 会把窗口激活（= 提到最前），
        // 于是「切主题」会顺手把主窗口顶到设置页前面，看起来像设置页自己关了。
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SWP_NOZORDER,
            };
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// 还原最大化 / 最小化的窗口。
pub fn restore_window(window: &gpui_kit::Window) -> bool {
    match window_hwnd_of(window) {
        Some(hwnd) => unsafe { ShowWindow(hwnd, SW_RESTORE) != 0 },
        None => false,
    }
}

/// 主窗口句柄缓存。
///
/// 关键点：窗口被 `SW_HIDE` 之后 `IsWindowVisible` 为假、枚举不到，
/// 但句柄本身仍然有效。所以要把它记下来 —— 否则 hide 之后既 show 不回来，
/// 连「关闭按钮」也会失灵（关闭 = 隐藏，隐藏失败看起来就是关不掉）。
fn cache() -> &'static std::sync::Mutex<Option<isize>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<isize>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// 找到当前进程的主窗口句柄：先看缓存，失效了再枚举可见窗口。
pub fn main_window() -> Option<*mut c_void> {
    if let Ok(guard) = cache().lock() {
        if let Some(raw) = *guard {
            let hwnd = raw as *mut c_void;
            if unsafe { IsWindow(hwnd) } != 0 {
                return Some(hwnd);
            }
        }
    }

    let mut search = Search {
        pid: std::process::id(),
        best: None,
    };
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as isize);
    }
    let found = search.best.map(|(_, hwnd)| hwnd);
    if let Some(hwnd) = found {
        if let Ok(mut guard) = cache().lock() {
            *guard = Some(hwnd as isize);
        }
    }
    found
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
            force_foreground(hwnd);
            true
        },
        None => false,
    }
}

/// 只保证「可见」，既不改 z-order 也不抢焦点。
///
/// 用于「主题切换后把窗口显隐恢复原样」：该在的就露出来，但绝不能把用户
/// 正在操作的那个窗口顶到后面（`SW_SHOW` 会激活窗口，`SW_SHOWNA` 不会）。
pub fn keep_visible_without_activate(window: &gpui_kit::Window) -> bool {
    match window_hwnd_of(window) {
        Some(hwnd) => unsafe {
            if IsWindowVisible(hwnd) != 0 {
                return true;
            }
            let command = if IsIconic(hwnd) != 0 {
                // 最小化还原没有「不激活」的版本，还原后把前台还给原来的窗口
                let foreground = GetForegroundWindow();
                let shown = ShowWindow(hwnd, SW_SHOWNA) != 0;
                if !foreground.is_null() {
                    SetForegroundWindow(foreground);
                }
                shown
            } else {
                ShowWindow(hwnd, SW_SHOWNA) != 0
            };
            command
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

/// 开始拖动窗口：返回「光标相对窗口左上角」的偏移。
///
/// 为什么不交给系统（`window_control_area(Drag)` / `start_window_move`）：
/// - `start_window_move` 在 Windows 后端是空实现（只有 Wayland/X11 有）；
/// - `window_control_area` 靠 WM_NCHITTEST，而 gpui 的命中判定用的是
///   「上一帧算好的 mouse_hit_test」，鼠标刚移到标题栏、还没画新帧时
///   系统问「这是不是标题栏」，答案永远是 HTCLIENT —— 于是按下去拖不动。
///   （实测：把光标停在标题栏两秒再 SendMessage(WM_NCHITTEST) 仍返回 1。）
/// 所以自己跟着光标挪窗口，逻辑简单且不受帧时序影响。
pub fn begin_window_drag(window: &gpui_kit::Window) -> Option<(f32, f32)> {
    let hwnd = window_hwnd_of(window)?;
    unsafe {
        let mut point: POINT = std::mem::zeroed();
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        // 最大化时不让自绘标题栏拖动（拖动语义是「还原」，系统那套更顺手）
        if IsZoomed(hwnd) != 0 {
            return None;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
        Some((
            (point.x - rect.left) as f32,
            (point.y - rect.top) as f32,
        ))
    }
}

/// 把主窗口挪到「光标 - 偏移」的位置（拖动中的每一帧调用）。
pub fn drag_window_to(offset: (f32, f32)) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };
    let Some(hwnd) = main_window() else {
        return;
    };
    unsafe {
        let mut point: POINT = std::mem::zeroed();
        if GetCursorPos(&mut point) == 0 {
            return;
        }
        let x = point.x - offset.0.round() as i32;
        let y = point.y - offset.1.round() as i32;
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
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
