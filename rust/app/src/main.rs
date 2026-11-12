//! 桌面端入口。

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

fn main() {
    // 单实例：已经有窗口在跑就提示并退出（不重复签到）
    let Some(_guard) = system::singleton::SingleInstance::acquire() else {
        eprintln!("班级魔方自动签到已经在运行，请从托盘图标打开。");
        return;
    };

    let paths = Paths::for_current_process();
    let _ = paths.ensure();
    let config_path = paths.config_file();
    let settings_path = paths.settings_file();
    let state = AppState::load(paths);

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
            title: Some("班级魔方自动签到".into()),
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
