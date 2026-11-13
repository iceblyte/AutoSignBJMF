//! 托盘图标与菜单（屏 07）。
//!
//! `tray-icon` 要求「创建图标的线程上有 win32 消息循环」，所以这里单开一个线程：
//! 它只负责消息泵与菜单事件，业务动作全部写回共享状态，由界面/调度线程消费。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::TrayIconEvent;
use tray_icon::{Icon, TrayIconBuilder};

use crate::state::LogKind;

/// 托盘句柄。真正让图标活着的是线程里那个变量，这里只留一个「还活着」的凭证。
pub struct TrayHandle {
    _marker: Arc<()>,
}

/// 菜单项动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenWindow,
    RunNow,
    TogglePause,
    ToggleAutostart,
    Settings,
    Quit,
}

// 菜单项 id 显式指定成 ASCII 常量。
// 之前用 MenuItem::new 让库自动生成 id，再拿 item.id() 去比 —— 点击事件里的 id
// 与创建时取到的对不上，于是所有选项都匹配失败（只有「退出」因为直接 exit 才像是生效）。
const ID_OPEN: &str = "bjmf-open";
const ID_RUN_NOW: &str = "bjmf-run-now";
const ID_PAUSE: &str = "bjmf-pause";
const ID_AUTOSTART: &str = "bjmf-autostart";
const ID_SETTINGS: &str = "bjmf-settings";
const ID_QUIT: &str = "bjmf-quit";

/// 把菜单 id 映射成动作（纯函数，可测）。
pub fn action_of(id: &str) -> Option<TrayAction> {
    match id {
        ID_OPEN => Some(TrayAction::OpenWindow),
        ID_RUN_NOW => Some(TrayAction::RunNow),
        ID_PAUSE => Some(TrayAction::TogglePause),
        ID_AUTOSTART => Some(TrayAction::ToggleAutostart),
        ID_SETTINGS => Some(TrayAction::Settings),
        ID_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

/// 托盘图标：程序 logo 的 32×32 位图。
///
/// 由 `rust/_tools/make_tray_icon.py` 从 `design/v2/assets/logo.png` 生成
/// （alpha 预乘缩放 + 四周留白），直接以原始 RGBA 嵌进二进制，运行时不依赖外部资源。
const TRAY_RGBA: &[u8] = include_bytes!("../../assets/tray-32.rgba");
const TRAY_SIZE: u32 = 32;

/// 返回图标位图（RGBA 顺序，行优先）。
pub fn icon_rgba() -> (Vec<u8>, u32, u32) {
    debug_assert_eq!(
        TRAY_RGBA.len(),
        (TRAY_SIZE * TRAY_SIZE * 4) as usize,
        "托盘位图尺寸不对，重新跑 rust/_tools/make_tray_icon.py"
    );
    (TRAY_RGBA.to_vec(), TRAY_SIZE, TRAY_SIZE)
}

/// 起托盘线程。`shared` 里的字段被菜单直接改写。
pub fn spawn(
    runtime: crate::runtime::SharedHandle,
    paused: Arc<std::sync::atomic::AtomicBool>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    run_now: Arc<std::sync::atomic::AtomicBool>,
    autostart_on: bool,
) -> Result<TrayHandle, String> {
    let marker = Arc::new(());
    let marker_thread = marker.clone();

    std::thread::Builder::new()
        .name("bjmf-tray".into())
        .spawn(move || {
            let _keep = marker_thread;
            match build_tray(autostart_on) {
                Ok((items, _tray)) => {
                    let mut last_tick = std::time::Instant::now();
                    // 菜单点击改用 set_event_handler 接收（与 ElegantClipboard 一致）。
                    // 之前是 MenuEvent::receiver() + try_recv 轮询 —— 事件根本到不了，
                    // 所以除了直接 exit 的「退出」，其它菜单项都像点了没反应。
                    {
                        let shared = runtime.clone();
                        let paused = paused.clone();
                        let run_now = run_now.clone();
                        MenuEvent::set_event_handler(Some(Box::new(move |event: MenuEvent| {
                            apply_action(action_of(event.id.0.as_str()), &shared, &paused, &run_now);
                        })));
                    }
                    {
                        let shared = runtime.clone();
                        TrayIconEvent::set_event_handler(Some(Box::new(
                            move |event: TrayIconEvent| {
                                // 菜单在右键，左键单击就直接把窗口叫出来
                                if let TrayIconEvent::Click {
                                    button: tray_icon::MouseButton::Left,
                                    button_state: tray_icon::MouseButtonState::Up,
                                    ..
                                } = event
                                {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.show_requested = true;
                                    }
                                }
                            },
                        )));
                    }

                    // 托盘线程仍然要有消息循环，否则菜单弹不出来
                    unsafe {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{
                            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
                        };
                        loop {
                            if stop.load(Ordering::Relaxed) {
                                break;
                            }
                            let mut message: MSG = std::mem::zeroed();
                            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE)
                                != 0
                            {
                                TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                            // 每 2 秒把「开机自启 / 暂停」两行的文字刷成真实状态，
                            // 免得设置页改了之后菜单文案还是启动时的快照
                            if last_tick.elapsed() >= std::time::Duration::from_secs(2) {
                                last_tick = std::time::Instant::now();
                                let autostart_on = crate::system::autostart::is_enabled();
                                items.autostart.set_text(if autostart_on {
                                    "开机自启（已开启）"
                                } else {
                                    "开机自启（已关闭）"
                                });
                                let is_paused = paused.load(Ordering::Relaxed);
                                items.pause.set_text(if is_paused {
                                    "恢复全部"
                                } else {
                                    "暂停全部"
                                });
                            }
                            std::thread::sleep(std::time::Duration::from_millis(80));
                        }
                    }
                }
                Err(error) => {
                    if let Ok(mut guard) = runtime.lock() {
                        guard
                            .pending
                            .push((LogKind::Warn, format!("托盘创建失败：{error}")));
                    }
                }
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(TrayHandle { _marker: marker })
}

/// 需要在运行期改文字的两个菜单项。
struct TrayItems {
    autostart: MenuItem,
    pause: MenuItem,
}

/// 菜单动作的实际效果：全部只写共享状态，真正的动作由界面/调度线程消费。
fn apply_action(
    action: Option<TrayAction>,
    shared: &crate::runtime::SharedHandle,
    paused: &Arc<std::sync::atomic::AtomicBool>,
    run_now: &Arc<std::sync::atomic::AtomicBool>,
) {
    match action {
        Some(TrayAction::OpenWindow) => {
            if let Ok(mut guard) = shared.lock() {
                guard.show_requested = true;
            }
        }
        Some(TrayAction::RunNow) => {
            run_now.store(true, Ordering::SeqCst);
            if let Ok(mut guard) = shared.lock() {
                guard
                    .pending
                    .push((LogKind::Info, "托盘：立即签到全部".into()));
            }
        }
        Some(TrayAction::TogglePause) => {
            let next = !paused.load(Ordering::Relaxed);
            paused.store(next, Ordering::SeqCst);
            if let Ok(mut guard) = shared.lock() {
                guard.paused = next;
                guard.pending.push((
                    LogKind::Info,
                    if next {
                        "托盘：已暂停全部".to_string()
                    } else {
                        "托盘：已恢复".to_string()
                    },
                ));
            }
        }
        Some(TrayAction::ToggleAutostart) => {
            let next = !crate::system::autostart::is_enabled();
            crate::system::autostart::set_enabled(next);
            if let Ok(mut guard) = shared.lock() {
                guard.pending.push((
                    LogKind::Info,
                    if next {
                        "托盘：已开启开机自启".to_string()
                    } else {
                        "托盘：已关闭开机自启".to_string()
                    },
                ));
            }
        }
        Some(TrayAction::Settings) => {
            if let Ok(mut guard) = shared.lock() {
                guard.show_requested = true;
                guard.show_settings = true;
                guard.pending.push((LogKind::Info, "托盘：打开设置".into()));
            }
        }
        Some(TrayAction::Quit) => std::process::exit(0),
        None => {}
    }
}

fn build_tray(autostart_on: bool) -> Result<(TrayItems, tray_icon::TrayIcon), String> {
    let open = MenuItem::with_id(ID_OPEN, "打开主窗口", true, None);
    let run_now = MenuItem::with_id(ID_RUN_NOW, "立即签到全部", true, None);
    let pause = MenuItem::with_id(ID_PAUSE, "暂停全部", true, None);
    let autostart = MenuItem::with_id(
        ID_AUTOSTART,
        if autostart_on {
            "开机自启（已开启）"
        } else {
            "开机自启"
        },
        true,
        None,
    );
    let settings = MenuItem::with_id(ID_SETTINGS, "设置", true, None);
    let quit = MenuItem::with_id(ID_QUIT, "退出", true, None);

    let items = TrayItems {
        autostart: autostart.clone(),
        pause: pause.clone(),
    };

    let menu = Menu::new();
    menu.append_items(&[
        &open,
        &run_now,
        &pause,
        &PredefinedMenuItem::separator(),
        &autostart,
        &settings,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .map_err(|error| error.to_string())?;

    let (rgba, width, height) = icon_rgba();
    let icon = Icon::from_rgba(rgba, width, height).map_err(|error| error.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        // 默认是「左键弹菜单」，那样右键反而没反应；改成右键出菜单、左键直接开窗口
        .with_menu_on_left_click(false)
        .with_tooltip("班级魔方自动签到")
        .with_icon(icon)
        .build()
        .map_err(|error| error.to_string())?;

    Ok((items, tray))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 图标尺寸与像素数匹配() {
        let (rgba, width, height) = icon_rgba();
        assert_eq!(width, 32);
        assert_eq!(height, 32);
        assert_eq!(rgba.len(), (width * height * 4) as usize);
    }

    #[test]
    fn 图标是_logo_的形状而不是纯色方块() {
        let (rgba, _, _) = icon_rgba();
        // logo 四周透明，左上角应当是空的
        assert_eq!(rgba[3], 0, "左上角应当透明");

        // 不透明像素要占到一定比例，否则说明贴图缩得太小、四周全是空白
        let opaque = (0..32 * 32).filter(|i| rgba[i * 4 + 3] > 200).count();
        let ratio = opaque as f32 / (32.0 * 32.0);
        assert!(
            ratio > 0.2,
            "不透明像素只有 {:.1}%，图标会显得很小",
            ratio * 100.0
        );

        // 四周应当有留白（不是把整张图拉满），否则贴边会很难看
        assert_eq!(rgba[3], 0, "上边框应当留白");
        assert_eq!(rgba[(31 * 32) * 4 + 3], 0, "左下角应当留白");
    }

    #[test]
    fn 菜单动作映射() {
        assert_eq!(action_of(ID_OPEN), Some(TrayAction::OpenWindow));
        assert_eq!(action_of(ID_RUN_NOW), Some(TrayAction::RunNow));
        assert_eq!(action_of(ID_PAUSE), Some(TrayAction::TogglePause));
        assert_eq!(action_of(ID_AUTOSTART), Some(TrayAction::ToggleAutostart));
        assert_eq!(action_of(ID_SETTINGS), Some(TrayAction::Settings));
        assert_eq!(action_of(ID_QUIT), Some(TrayAction::Quit));
        assert_eq!(action_of("99"), None, "未知 id 不能被误判");
    }

    #[test]
    fn 菜单动作会真的写到共享状态() {
        use crate::runtime::Shared;
        use std::sync::atomic::AtomicBool;
        use std::sync::Mutex;

        let shared = Arc::new(Mutex::new(Shared::default()));
        let paused = Arc::new(AtomicBool::new(false));
        let run_now = Arc::new(AtomicBool::new(false));

        apply_action(Some(TrayAction::RunNow), &shared, &paused, &run_now);
        assert!(run_now.load(Ordering::SeqCst), "立即签到要置位");

        apply_action(Some(TrayAction::TogglePause), &shared, &paused, &run_now);
        assert!(paused.load(Ordering::SeqCst), "暂停要真的切换");
        assert!(shared.lock().unwrap().paused, "界面读到的也是暂停");

        apply_action(Some(TrayAction::OpenWindow), &shared, &paused, &run_now);
        assert!(
            shared.lock().unwrap().show_requested,
            "打开窗口要置请求，否则点了没反应"
        );

        apply_action(Some(TrayAction::Settings), &shared, &paused, &run_now);
        assert!(shared.lock().unwrap().show_settings, "设置要额外带上标记");
    }
}
