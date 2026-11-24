//! 桌面端入口。

// 双击运行时不弹控制台窗口。
// release 用 Windows 子系统（无控制台），debug / test 保留控制台，方便看 panic 与测试输出。
#![cfg_attr(
    all(not(debug_assertions), not(test)),
    windows_subsystem = "windows"
)]

mod assets;
mod boot;

/// 界面侧诊断日志：写到临时目录，release（无控制台）也能事后查。
pub fn ui_debug(message: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("bjmf-ui-debug.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{message}");
    }
}
mod runtime;
mod state;
mod system;
mod theme;
mod ui;

use std::sync::Arc;

use bjmf_core::Paths;
use gpui_kit::*;

use crate::assets::Assets;
use crate::runtime::Runtime;
use crate::state::AppState;
use crate::ui::AppView;

/// 主窗口标题。单实例冲突时靠它找到已运行的窗口。
///
/// 统一取 `identity::APP_NAME`：任务栏、托盘、窗口标题、vbs 激活都用同一个串，
/// 避免哪天改了一处、别处还留着旧名字。
pub const WINDOW_TITLE: &str = system::identity::APP_NAME;

fn main() {
    // 单实例：已经有窗口在跑就把它叫到前面，然后退出（不重复签到）。
    // 注意这里是 GUI 子系统，println/eprintln 看不到，所以用窗口激活 + MessageBox 兜底。
    // QA/自动化场景允许跳过单实例检查，避免和用户正在运行的实例抢锁
    let skip_single = std::env::var_os("AUTOSIGN_NO_SINGLE").is_some();
    let _guard = if skip_single {
        None
    } else {
        Some(system::singleton::SingleInstance::acquire())
    };
    if !skip_single && _guard.as_ref().map(|guard| guard.is_none()).unwrap_or(false) {
        #[cfg(debug_assertions)]
        eprintln!("{WINDOW_TITLE} 已经在运行，请从托盘图标打开。");
        if !system::win::activate_window_by_title(WINDOW_TITLE) {
            // 窗口找不到（可能已被隐藏/最小化到托盘），至少给用户一个看得见的提示
            system::win::message_box(
                WINDOW_TITLE,
                "程序已经在运行。\n窗口可能被最小化到了托盘，请在任务栏右下角的托盘图标里找一下。",
            );
        }
        return;
    }

    let paths = Paths::for_current_process();
    let _ = paths.ensure();
    let config_path = paths.config_file();
    let settings_path = paths.settings_file();
    let mut state = AppState::load(paths);
    // 记一条启动日志：落盘后用户下次打开还能看到上次什么时候跑过
    state.log(
        crate::state::LogKind::Info,
        format!("程序启动，共 {} 个账号", state.config.students.len()),
    );

    // 开机自启：注册表是**本机**的，设置是跟着 data 目录走的（便携版会换机器、
    // 换盘符），所以每次启动按设置对齐一次。不这样做就会出现「界面显示已开启、
    // 开机其实没起来」——老版本只在注册表已有值时才改写，正是这个问题。
    match system::autostart::sync(state.settings.autostart) {
        system::autostart::Synced::Written => state.log(
            state::LogKind::Info,
            "开机自启：已写入本机启动项（指向当前 exe，--tray 静默启动）",
        ),
        system::autostart::Synced::Removed => {
            state.log(state::LogKind::Info, "开机自启：已从本机启动项移除")
        }
        system::autostart::Synced::Foreign => state.log(
            state::LogKind::Warn,
            "开机自启：注册表里的启动项指向另一份程序，未改动",
        ),
        system::autostart::Synced::Failed => state.log(
            state::LogKind::Warn,
            "开机自启：写注册表失败（可能被安全软件或组策略拦住）",
        ),
        system::autostart::Synced::UpToDate | system::autostart::Synced::Absent => {}
    }

    // 应用身份：显式 AppID + 注册显示名（必须在第一个窗口创建之前）
    system::identity::install();
    if let Some(warning) = system::identity::self_check() {
        state.log(crate::state::LogKind::Warn, warning);
    }

    // 后台：调度线程 + 托盘
    // 托盘菜单的初始文案取**注册表实际状态**，别拿 json 里的意愿冒充
    let runtime = Arc::new(Runtime::start(
        config_path,
        settings_path,
        system::autostart::is_active(),
    ));

    // 开机自启（注册表里带 --tray）或显式要求：不建窗口，只跑托盘 + 调度。
    // 这样开机那一刻不会初始化 D3D，第一次点托盘才建窗。
    let tray_only = std::env::var_os("AUTOSIGN_TRAY_ONLY").is_some()
        || std::env::args().any(|arg| arg == "--tray" || arg == "--silent");

    let mut app = application().with_assets(Assets);
    if tray_only {
        // headless 起步：平台不 attach GPU（设备 / 图集 / 交换链一个都不占）
        app = app.with_windowing(gpui_kit::WindowingRequest::Headless);
    }

    app.run(move |cx| {
        gpui_kit::init(cx);
        // 托盘模式下启动时一个窗口都没有，gpui 默认会当成"该退出了"
        cx.set_quit_mode(gpui_kit::QuitMode::Explicit);

        if tray_only {
            boot::tray_only(state, runtime, cx);
        } else {
            boot::open_main_window(state, runtime, cx);
        }
    });
}
