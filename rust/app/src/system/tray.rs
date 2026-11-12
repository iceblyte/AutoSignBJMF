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

/// 把菜单 id 映射成动作（纯函数，可测）。
pub fn action_of(id: &str, ids: &TrayIds) -> Option<TrayAction> {
    if id == ids.open {
        Some(TrayAction::OpenWindow)
    } else if id == ids.run_now {
        Some(TrayAction::RunNow)
    } else if id == ids.pause {
        Some(TrayAction::TogglePause)
    } else if id == ids.autostart {
        Some(TrayAction::ToggleAutostart)
    } else if id == ids.settings {
        Some(TrayAction::Settings)
    } else if id == ids.quit {
        Some(TrayAction::Quit)
    } else {
        None
    }
}

/// 菜单项 id 集合。
#[derive(Debug, Clone, Default)]
pub struct TrayIds {
    pub open: String,
    pub run_now: String,
    pub pause: String,
    pub autostart: String,
    pub settings: String,
    pub quit: String,
}

/// 生成图标位图：蓝底圆角 + 白色对勾（与托盘截图一致，不依赖外部资源）。
pub fn icon_rgba() -> (Vec<u8>, u32, u32) {
    const S: usize = 32;
    let mut buffer = vec![0u8; S * S * 4];
    let radius = 7.0f32;
    for y in 0..S {
        for x in 0..S {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let cx = fx.clamp(radius, S as f32 - radius);
            let cy = fy.clamp(radius, S as f32 - radius);
            let distance = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            if distance > radius {
                continue;
            }
            let index = (y * S + x) * 4;
            let segment = |ax: f32, ay: f32, bx: f32, by: f32| {
                let (px, py) = (fx - ax, fy - ay);
                let (vx, vy) = (bx - ax, by - ay);
                let t = ((px * vx + py * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
                ((px - t * vx).powi(2) + (py - t * vy).powi(2)).sqrt()
            };
            let on_check =
                segment(9.0, 17.0, 14.0, 22.0) < 2.0 || segment(14.0, 22.0, 23.0, 11.0) < 2.0;
            if on_check {
                buffer[index..index + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else {
                buffer[index..index + 4].copy_from_slice(&[0x25, 0x63, 0xeb, 255]);
            }
        }
    }
    (buffer, S as u32, S as u32)
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
                Ok((ids, items, _tray)) => {
                    let mut last_tick = std::time::Instant::now();
                    let receiver = MenuEvent::receiver();
                    let icon_events = TrayIconEvent::receiver();
                    // 托盘线程要有消息循环，菜单才会弹出来；
                    // 用 PeekMessage 非阻塞轮询，顺便处理菜单事件与退出标志。
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
                            // 双击托盘图标 = 打开主窗口
                            while let Ok(event) = icon_events.try_recv() {
                                if let TrayIconEvent::DoubleClick { button, .. } = event {
                                    if matches!(button, tray_icon::MouseButton::Left) {
                                        if let Ok(mut guard) = runtime.lock() {
                                            guard.show_requested = true;
                                        }
                                    }
                                }
                            }
                            while let Ok(event) = receiver.try_recv() {
                                match action_of(event.id.0.as_str(), &ids) {
                                    Some(TrayAction::OpenWindow) => {
                                        if let Ok(mut guard) = runtime.lock() {
                                            guard.show_requested = true;
                                        }
                                    }
                                    Some(TrayAction::RunNow) => {
                                        run_now.store(true, Ordering::SeqCst);
                                        if let Ok(mut guard) = runtime.lock() {
                                            guard.pending.push((
                                                LogKind::Info,
                                                "托盘：立即签到全部".into(),
                                            ));
                                        }
                                    }
                                    Some(TrayAction::TogglePause) => {
                                        let next = !paused.load(Ordering::Relaxed);
                                        paused.store(next, Ordering::SeqCst);
                                        if let Ok(mut guard) = runtime.lock() {
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
                                        if let Ok(mut guard) = runtime.lock() {
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
                                        if let Ok(mut guard) = runtime.lock() {
                                            guard.show_requested = true;
                                            guard.pending.push((
                                                LogKind::Info,
                                                "托盘：打开设置".into(),
                                            ));
                                        }
                                    }
                                    Some(TrayAction::Quit) => {
                                        stop.store(true, Ordering::SeqCst);
                                        std::process::exit(0);
                                    }
                                    None => {}
                                }
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

fn build_tray(autostart_on: bool) -> Result<(TrayIds, TrayItems, tray_icon::TrayIcon), String> {
    let open = MenuItem::new("打开主窗口", true, None);
    let run_now = MenuItem::new("立即签到全部", true, None);
    let pause = MenuItem::new("暂停全部", true, None);
    let autostart = MenuItem::new(
        if autostart_on {
            "开机自启（已开启）"
        } else {
            "开机自启"
        },
        true,
        None,
    );
    let settings = MenuItem::new("设置", true, None);
    let quit = MenuItem::new("退出", true, None);

    let items = TrayItems {
        autostart: autostart.clone(),
        pause: pause.clone(),
    };

    let items = TrayItems {
        autostart: autostart.clone(),
        pause: pause.clone(),
    };

    let ids = TrayIds {
        open: open.id().0.clone(),
        run_now: run_now.id().0.clone(),
        pause: pause.id().0.clone(),
        autostart: autostart.id().0.clone(),
        settings: settings.id().0.clone(),
        quit: quit.id().0.clone(),
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
        .with_tooltip("班级魔方自动签到")
        .with_icon(icon)
        .build()
        .map_err(|error| error.to_string())?;

    Ok((ids, items, tray))
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
    fn 图标四角透明中心有颜色() {
        let (rgba, _, _) = icon_rgba();
        // 左上角在圆角外 → alpha 0
        assert_eq!(rgba[3], 0);
        // 中心是勾或底色，alpha 255
        let center = (16 * 32 + 16) * 4;
        assert_eq!(rgba[center + 3], 255);
    }

    #[test]
    fn 菜单动作映射() {
        let ids = TrayIds {
            open: "1".into(),
            run_now: "2".into(),
            pause: "3".into(),
            autostart: "4".into(),
            settings: "5".into(),
            quit: "6".into(),
        };
        assert_eq!(action_of("1", &ids), Some(TrayAction::OpenWindow));
        assert_eq!(action_of("3", &ids), Some(TrayAction::TogglePause));
        assert_eq!(action_of("6", &ids), Some(TrayAction::Quit));
        assert_eq!(action_of("99", &ids), None);
    }
}
