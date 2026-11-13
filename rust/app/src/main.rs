//! 桌面端入口。

// 双击运行时不弹控制台窗口。
// release 用 Windows 子系统（无控制台），debug / test 保留控制台，方便看 panic 与测试输出。
#![cfg_attr(
    all(not(debug_assertions), not(test)),
    windows_subsystem = "windows"
)]

mod assets;
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
pub const WINDOW_TITLE: &str = "班级魔方自动签到";

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

    // 后台：调度线程 + 托盘
    let runtime = Arc::new(Runtime::start(
        config_path,
        settings_path,
        state.settings.autostart,
    ));

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: size(px(theme::WINDOW_W), px(theme::WINDOW_H)),
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(WINDOW_TITLE.into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_min_size: Some(size(px(980.0), px(640.0))),
        ..Default::default()
    };

    application().with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        let handle = open_window(options, cx, move |_, cx| {
            cx.new(|cx| {
                let mut view = AppView::new(state, runtime.clone(), cx);
                view.start_ticker(cx);
                view
            })
        })
        .expect("打开主窗口失败");

        // 关窗到托盘：拦下关闭、只提示一次（真正隐藏窗口需要平台层 API，
        // 这一版先做到「不会因为关窗把调度停掉」）。
        let signal = handle.1.read(cx).close_signal.clone();
        let _ = handle.0.update(cx, move |_view, window, cx| {
            window.on_window_should_close(cx, move |_window, _cx| {
                signal.store(true, std::sync::atomic::Ordering::SeqCst);
                false
            });
        });
    });
}
