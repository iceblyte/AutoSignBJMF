//! 窗口的诞生方式：正常启动直接开窗；开机自启进「托盘模式」，第一次点托盘才开窗。
//!
//! 托盘模式的意义是**开机时不碰 GPU**：不建窗口 → 平台不会 `attach_gpu`，
//! D3D 设备、字形图集、交换链一个都不分配。开机那一刻的常驻内存因此只有
//! 调度线程 + 托盘图标（几 MB），第一次点托盘时才付出建窗成本。
//!
//! 两条硬约束（踩过才知道）：
//! 1. 没有窗口时 gpui 默认会认为「该退出了」，必须 `QuitMode::Explicit`；
//! 2. `request_windowing` 要求**当前没有任何窗口**，所以顺序只能是
//!    「先切 Windowed（重新 attach GPU）→ 再 open_window」。

use std::sync::Arc;

use gpui_kit::*;

use crate::runtime::Runtime;
use crate::state::AppState;
use crate::system::tray::TrayCommand;
use crate::theme;
use crate::ui::AppView;
use crate::WINDOW_TITLE;

thread_local! {
    /// `AppView` 的强引用。
    ///
    /// 窗口销毁后，window 的 root view 是唯一的强引用 —— 一拆窗口，实体就被回收，
    /// 托盘命令、tick、日志全都没了（表现：点托盘没反应）。这里留一份，
    /// 让「窗口没了但应用还在」这条状态成立。
    static MAIN_VIEW: std::cell::RefCell<Option<Entity<AppView>>> =
        const { std::cell::RefCell::new(None) };
}

/// 记住实体（建窗后调用一次；重建窗口时继续沿用同一个）。
fn remember_view(entity: &Entity<AppView>) {
    MAIN_VIEW.with(|slot| *slot.borrow_mut() = Some(entity.clone()));
}

/// 取回实体（没有窗口时也能拿到）。
fn main_view() -> Option<Entity<AppView>> {
    MAIN_VIEW.with(|slot| slot.borrow().clone())
}

/// 主窗口的外观（启动与「从托盘唤起」共用，两边必须一致）。
///
/// `bounds` 是上次销毁前记住的位置/大小（含最大化）；`None` 用默认的 120,80。
fn window_options(bounds: Option<WindowBounds>) -> WindowOptions {
    let bounds = bounds.unwrap_or_else(|| {
        WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: size(px(theme::WINDOW_W), px(theme::WINDOW_H)),
        })
    });
    WindowOptions {
        window_bounds: Some(bounds),
        // 客户端装饰：不要系统标题栏，整条标题栏由我们自己画
        // （否则深色模式下系统会盖一层浅色背景，怎么调 DWM 都不对）
        window_decorations: Some(gpui_kit::WindowDecorations::Client),
        titlebar: Some(TitlebarOptions {
            // 标题仍要有：任务栏与截图工具都靠它认窗口（客户端装饰下不会画出来）
            title: Some(WINDOW_TITLE.into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        // 最小宽度要 ≤ 半屏，否则「拖到屏幕左/右边缘分屏」会被系统判成放不下、
        // 直接钳到最小尺寸（1920 屏半屏是 960，所以这里给 860；1366 的笔记本半屏
        // 只有 683，分屏仍会被钳 —— 再窄排版就散了，不追这个）。
        window_min_size: Some(Size::new(px(860.0), px(560.0))),
        ..Default::default()
    }
}

/// 建主窗口（含「关窗到托盘」的拦截钩子）。
pub fn open_main_window(
    state: AppState,
    runtime: Arc<Runtime>,
    cx: &mut App,
) -> (AnyWindowHandle, Entity<AppView>) {
    let handle = open_window(window_options(None), cx, move |window, cx| {
        // 同一个 entity 会被两个窗口渲染（主窗口 / 设置窗口），
        // 记下主窗口的 id，界面靠它决定画哪一套
        let main_window_id = window.window_handle().window_id();
        cx.new(|cx| {
            let mut view = AppView::new(state, runtime.clone(), main_window_id, window, cx);
            view.start_ticker(cx);
            view
        })
    })
    .expect("打开主窗口失败");

    // 窗口销毁后实体还得活着（否则托盘再点就找不到界面了）
    remember_view(&handle.1);
    hook_close(&handle, &handle.1, cx);
    handle
}

/// 托盘模式：一个窗口都不建，等托盘里的第一条「需要界面」的命令再建。
pub fn tray_only(state: AppState, runtime: Arc<Runtime>, cx: &mut App) {
    let Some(receiver) = runtime.take_tray_events() else {
        // 托盘没起来（QA 里禁用托盘时）—— 那这个模式没意义，直接开窗
        open_main_window(state, runtime, cx);
        return;
    };

    crate::ui_debug("tray_only: 托盘模式启动，未创建窗口");

    // QA：托盘模式下没有界面，main_view 里那个「模拟点托盘」的钩子不会跑，
    // 这里补一条，直接往托盘通道塞命令（默认 Open）。
    if let Ok(which) = std::env::var("AUTOSIGN_TEST_TRAY") {
        let command = match which.as_str() {
            "settings" => TrayCommand::Settings,
            "runnow" => TrayCommand::RunNow,
            "pause" => TrayCommand::TogglePause,
            _ => TrayCommand::Open,
        };
        let delay = std::env::var("AUTOSIGN_TEST_TRAY_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(3000);
        let runtime = runtime.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(delay))
                .await;
            runtime.send_tray_command(command);
            // 再发一条：建完窗口后这条应该由界面（而不是启动逻辑）收到，
            // 用来验证「通道交回界面」这一步没漏
            cx.background_executor()
                .timer(std::time::Duration::from_secs(6))
                .await;
            runtime.send_tray_command(command);
        })
        .detach();
    }

    let mut state = Some(state);
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(command) = receiver.recv().await {
            crate::ui_debug(&format!("tray_only: 收到 {command:?}"));
            match command {
                // 只有这两条需要界面：建窗口（并切回 windowed，重新 attach GPU）
                TrayCommand::Open | TrayCommand::Settings => {
                    let want_settings = matches!(command, TrayCommand::Settings);
                    let task = cx.update(|app| {
                        app.request_windowing(gpui_kit::WindowingRequest::Windowed(
                            gpui_kit::GraphicalEnvironment::detect(),
                        ))
                    });
                    if let Err(error) = task.await {
                        crate::ui_debug(&format!("tray_only: 切 windowed 失败 {error}"));
                    }

                    // 剩下的命令交回给界面（通道是同一个，中间进来的不会丢）
                    runtime.restore_tray_events(receiver.clone());

                    let Some(state) = state.take() else {
                        break;
                    };
                    let opened = cx.update(|app| open_main_window(state, runtime.clone(), app));
                    let entity = opened.1;
                    if want_settings {
                        cx.update(|app| {
                            entity.update(app, |view, cx| view.open_settings(cx));
                        });
                    }
                    break;
                }
                // 不需要界面的命令：直接打到运行时，行为和界面在时一致
                TrayCommand::RunNow => {
                    runtime.request_run_now();
                }
                TrayCommand::TogglePause => {
                    let (_, paused, _, _) = runtime.snapshot();
                    let next = !paused;
                    runtime.set_paused(next);
                    crate::ui_debug(&format!("tray_only: 暂停 → {next}"));
                }
                TrayCommand::ToggleAutostart => {
                    let next = !crate::system::autostart::is_enabled();
                    crate::system::autostart::set_enabled(next);
                    crate::ui_debug(&format!("tray_only: 开机自启 → {next}"));
                }
                TrayCommand::Quit => {
                    cx.update(|app| app.quit());
                    break;
                }
            }
        }
    })
    .detach();
}

/// 从托盘恢复：先把平台切回 windowed（重新 `attach_gpu`），再按记住的几何重建窗口。
///
/// 顺序不能反 —— `open_window` 要在 GPU 已经 attach 之后才拿得到渲染器。
pub fn restore_main_window(cx: &mut Context<AppView>) {
    let Some(entity) = main_view() else {
        crate::ui_debug("restore_main_window: 没有实体引用（不应该发生）");
        return;
    };
    cx.spawn(async move |_this, cx| {
        switch_to_windowed(cx).await;
        cx.update(|app| {
            reopen_main_window(entity, app);
        });
    })
    .detach();
}

/// 只需要设置页时：切回 windowed 后直接开设置窗口，不弹主界面。
pub fn restore_settings_only(cx: &mut Context<AppView>) {
    let Some(entity) = main_view() else {
        crate::ui_debug("restore_settings_only: 没有实体引用（不应该发生）");
        return;
    };
    cx.spawn(async move |_this, cx| {
        switch_to_windowed(cx).await;
        cx.update(|app| {
            entity.update(app, |view, cx| view.open_settings(cx));
        });
    })
    .detach();
}

/// 切到 windowed（已经是的话会返回 "already windowed"，忽略）。
async fn switch_to_windowed(cx: &mut AsyncApp) {
    let task = cx.update(|app| {
        app.request_windowing(gpui_kit::WindowingRequest::Windowed(
            gpui_kit::GraphicalEnvironment::detect(),
        ))
    });
    match task.await {
        Ok(()) => crate::ui_debug("switch_to_windowed: GPU 已重新 attach"),
        Err(error) => crate::ui_debug(&format!("switch_to_windowed: {error}")),
    }
}

/// 用已有的 `AppView` 实体重建主窗口（实体活着，只换窗口）。
pub fn reopen_main_window(entity: Entity<AppView>, cx: &mut App) -> AnyWindowHandle {
    let bounds = entity.read(cx).saved_bounds.clone();
    let handle = open_window(window_options(bounds), cx, {
        let entity = entity.clone();
        move |window, cx| {
            let window_id = window.window_handle().window_id();
            entity.update(cx, |view, cx| view.attach_to_window(window_id, window, cx));
            entity.clone()
        }
    })
    .expect("重建主窗口失败");
    crate::ui_debug("reopen_main_window: 窗口已重建");
    remember_view(&entity);
    hook_close(&handle, &entity, cx);
    handle.0
}

/// 挂「用户点关闭」的拦截（重建后必须重挂，旧的钩子随窗口没了）。
fn hook_close(
    handle: &(AnyWindowHandle, Entity<AppView>),
    entity: &Entity<AppView>,
    cx: &mut App,
) {
    let signal = entity.read(cx).close_signal.clone();
    let _ = handle.0.update(cx, move |_view, window, cx| {
        window.on_window_should_close(cx, move |_window, _cx| {
            signal.store(true, std::sync::atomic::Ordering::SeqCst);
            false
        });
    });
}
