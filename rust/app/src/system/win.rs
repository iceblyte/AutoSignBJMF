//! 窗口显隐：拿到本进程主窗口的 HWND，然后真正地隐藏 / 唤起。
//!
//! gpui 0.3 没有暴露 hide 接口，只能从平台层补：按进程 id 枚举顶层窗口，
//! 取面积最大的那个可见窗口当作主窗口。

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetCursorPos, GetForegroundWindow, GetWindow, GetWindowRect,
    GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, IsZoomed, MessageBoxW, SetForegroundWindow, ShowWindow,
    ShowWindowAsync, GW_OWNER, MB_ICONINFORMATION, MB_OK, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNA,
    SW_SHOWMAXIMIZED,
};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, WM_NCHITTEST,
};

/// 转成 Windows 需要的以 0 结尾的 UTF-16。
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 进入后台态：把工作集还给系统 + 把内存优先级降到最低。
///
/// 这是一个托盘常驻应用，隐藏后用户看不到任何东西，没必要占着几十 MB 常驻内存：
/// - `SetProcessWorkingSetSizeEx(.., -1, -1, 0)`：把工作集页标记为空闲，
///   随时可被回收/换出（commit 不变，再打开时按需缺页回来，代价是首帧略慢）；
/// - `SetProcessInformation(ProcessMemoryPriority, LOWEST)`：让系统在内存紧张时
///   优先把我们换出去，而不是等系统自己去压。
///
/// 两者都不影响正确性，只是「闲置时把资源让给别人」。
pub fn enter_background() {
    use windows_sys::Win32::System::Memory::SetProcessWorkingSetSizeEx;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, SetProcessInformation, MEMORY_PRIORITY_INFORMATION,
        MEMORY_PRIORITY_VERY_LOW, ProcessMemoryPriority,
    };
    unsafe {
        let process = GetCurrentProcess();
        let info = MEMORY_PRIORITY_INFORMATION {
            MemoryPriority: MEMORY_PRIORITY_VERY_LOW,
        };
        SetProcessInformation(
            process,
            ProcessMemoryPriority,
            &info as *const _ as *const c_void,
            std::mem::size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
        // -1 / -1 表示「按系统建议值」，等价于 EmptyWorkingSet
        let trimmed = SetProcessWorkingSetSizeEx(process, usize::MAX, usize::MAX, 0);
        crate::ui_debug(&format!("enter_background: trim={trimmed}"));
    }
}

/// 回到前台态：内存优先级恢复默认（工作集会被缺页慢慢填回来，不用管）。
pub fn leave_background() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, SetProcessInformation, MEMORY_PRIORITY_INFORMATION,
        MEMORY_PRIORITY_NORMAL, ProcessMemoryPriority,
    };
    unsafe {
        let info = MEMORY_PRIORITY_INFORMATION {
            MemoryPriority: MEMORY_PRIORITY_NORMAL,
        };
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessMemoryPriority,
            &info as *const _ as *const c_void,
            std::mem::size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
    }
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
    hide_window_with(window, false, false)
}

/// 隐藏主窗口，并把它记进唤醒缓存。
///
/// 只有主窗口能写缓存 —— 设置窗口也走 hide 的话，缓存会被它覆盖，
/// 之后托盘「打开主窗口」就会把设置窗口叫出来。
pub fn hide_main_window_of(window: &gpui_kit::Window) -> bool {
    // 主窗口藏起来 = 程序进入后台常驻：顺手把内存让出去
    hide_window_with(window, true, true)
}

fn hide_window_with(window: &gpui_kit::Window, remember: bool, background: bool) -> bool {
    let Some(hwnd) = window_hwnd_of(window) else {
        return false;
    };
    if remember {
        // 窗口一旦隐藏，EnumWindows 就找不到它了，唤回只能靠这个句柄
        if let Ok(mut guard) = cache().lock() {
            *guard = Some(hwnd as isize);
        }
    }
    let ok = unsafe { ShowWindow(hwnd, SW_HIDE) != 0 };
    if background {
        enter_background();
    }
    ok
}

/// 把指定窗口显示出来，最小化的先还原。
pub fn show_window(window: &gpui_kit::Window) -> bool {
    leave_background();
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
        Some(hwnd) => unsafe {
            let ok = ShowWindow(hwnd, SW_HIDE) != 0;
            // 藏起来之后立刻把内存让出去
            enter_background();
            ok
        },
        None => false,
    }
}

/// 重新显示并把窗口提到前台；最大化状态会被保留。
pub fn show_main_window() -> bool {
    leave_background();
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


pub fn drag_window_to(window: &gpui_kit::Window, offset: (f32, f32)) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };
    // 用「当前这个窗口」的 HWND，不是缓存的主窗口：设置窗口也是自绘标题栏
    let Some(hwnd) = window_hwnd_of(window) else {
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

// ---------------------------------------------------------------------------
// 自绘标题栏 + 系统边框：把 WM_NCHITTEST 接管回来
// ---------------------------------------------------------------------------
//
// 为什么需要：
// - gpui 在「客户端装饰」下只把**顶边**做成可拉伸（`handle_hit_test_msg` 里只有
//   HTTOP / HTTOPLEFT / HTTOPRIGHT），左右和下边只剩 DefWindowProc 的 1px，
//   实际用起来就是「边拉不动、很难抓住」；
// - 拖动 / 双击最大化 / Aero Snap / 标题栏右键系统菜单，系统只认 WM_NCHITTEST
//   返回的 HTCAPTION。gpui 的 `window_control_area` 依赖上一帧 hit test（不可用），
//   所以我们以前自己跟光标 SetWindowPos —— 代价就是上面那串系统行为全丢了。
//
// 做法：给 HWND 挂一个子类过程，在 WM_NCHITTEST 里回答两件事：
//   1) 四周 6 DIP 的条带 → HTLEFT/HTRIGHT/HTTOP/HTBOTTOM/四个角（拉伸，光标也会变）；
//   2) 标题栏条带（扣掉右边两个按钮）→ HTCAPTION（拖动 + 双击最大化 + snap + 系统菜单）。
// 其余一律 HTCLIENT，交回 gpui 正常处理（按钮、输入框都走这条）。

/// comctl32 的子类化函数：windows-sys 没导出，自己声明（标准做法）。
type SubclassProc =
    Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT>;

#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(
        hwnd: HWND,
        pfn_subclass: SubclassProc,
        id_subclass: usize,
        ref_data: usize,
    ) -> BOOL;
    fn DefSubclassProc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
}

/// 子类 id：一个窗口只挂一次。
const FRAME_SUBCLASS_ID: usize = 0x8A11;
/// 边框抓取带宽（DIP）。
const RESIZE_BAND_DIP: f32 = 6.0;

unsafe extern "system" fn frame_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        let hit = frame_hit_test(hwnd, lparam);
        if hit != HTCLIENT as isize {
            return hit;
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// 命中判定：先边框，再标题栏，最后交回客户端。
unsafe fn frame_hit_test(hwnd: HWND, lparam: LPARAM) -> isize {
    let mut point = POINT {
        x: (lparam & 0xffff) as u16 as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as u16 as i16 as i32,
    };
    ScreenToClient(hwnd, &mut point);

    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(hwnd, &mut client) == 0 {
        return HTCLIENT as isize;
    }

    let scale = (GetDpiForWindow(hwnd) as f32 / 96.0).max(1.0);
    let band = (RESIZE_BAND_DIP * scale).round() as i32;

    // 最大化时不要边框（那一圈贴着屏幕边），但标题栏仍要能拖 / 双击还原
    if IsZoomed(hwnd) == 0 {
        let left = point.x < band;
        let right = point.x >= client.right - band;
        let top = point.y < band;
        let bottom = point.y >= client.bottom - band;
        let edge = match (left, right, top, bottom) {
            (true, _, true, _) => HTTOPLEFT,
            (_, true, true, _) => HTTOPRIGHT,
            (true, _, _, true) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, ..) => HTLEFT,
            (_, true, ..) => HTRIGHT,
            (_, _, true, _) => HTTOP,
            (_, _, _, true) => HTBOTTOM,
            _ => 0,
        };
        if edge != 0 {
            return edge as isize;
        }
    }

    // 标题栏条带：右边两个窗口按钮留给 gpui 点，其余算标题栏
    let title_h = (crate::theme::TITLEBAR_H * scale).round() as i32;
    let buttons_w = (crate::theme::WINDOW_BTN_W * 2.0 * scale).round() as i32;
    if point.y < title_h && point.x < client.right - buttons_w {
        return HTCAPTION as isize;
    }

    HTCLIENT as isize
}

/// 给窗口装上「系统接管边框 + 标题栏」的子类过程。主窗口和设置窗口都要装。
pub fn install_frame_subclass(window: &gpui_kit::Window) {
    let Some(hwnd) = window_hwnd_of(window) else {
        return;
    };
    unsafe {
        let ok = SetWindowSubclass(hwnd, Some(frame_subclass), FRAME_SUBCLASS_ID, 0);

        // gpui 建窗时把 WS_CAPTION 摘掉了（自己画标题栏），但 Windows 的 Aero Snap
        // 只对「有标题栏的普通窗口」生效 —— 拖到屏幕顶端不最大化就是这个原因。
        // 把 WS_CAPTION 加回去：客户区尺寸不受影响（gpui 在 WM_NCCALCSIZE 里把
        // 上边距抹平了），但窗口重新被系统认成「可吸附」。
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_STYLE, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_CAPTION,
        };
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_CAPTION != WS_CAPTION {
            SetWindowLongW(hwnd, GWL_STYLE, (style | WS_CAPTION) as i32);
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
        crate::ui_debug(&format!("install_frame_subclass: ok={ok} style_before={style:#x}"));
    }
}
