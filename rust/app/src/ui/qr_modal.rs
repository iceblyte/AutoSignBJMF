//! 扫码添加账号（屏 03）：模态 + 后台拉码/轮询。
//!
//! 网络动作放线程里，界面只轮询一个很小的共享结构，避免在渲染里等 IO。

use std::sync::{Arc, Mutex};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, rgb, Context, SharedString, WeakEntity, Window};

use bjmf_core::QrLogin;

use crate::state::{Draft, LogKind};
use crate::theme::*;
use crate::theme::{DARK, LIGHT, Palette};
use crate::ui::main_view::AppView;
use crate::ui::widgets::dot;
use crate::ui::widgets::*;

/// 扫码阶段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrPhase {
    /// 正在拉二维码
    Loading,
    /// 已出码，等待扫码
    Waiting,
    /// 扫码成功，已写入账号
    Done { name: String },
    /// 失败
    Failed(String),
}

/// 后台线程与界面共享的一小块。
#[derive(Default)]
pub struct QrShared {
    pub phase: Option<QrPhase>,
    pub image: Option<String>,
    pub account: Option<(String, String, String)>,
}

pub type QrHandle = Arc<Mutex<QrShared>>;

/// 起一次扫码会话。
pub fn start(config_path: std::path::PathBuf) -> QrHandle {
    let shared: QrHandle = Arc::new(Mutex::new(QrShared {
        phase: Some(QrPhase::Loading),
        ..QrShared::default()
    }));
    let handle = shared.clone();

    std::thread::Builder::new()
        .name("bjmf-qr".into())
        .spawn(move || {
            let login = match QrLogin::production() {
                Ok(login) => login,
                Err(error) => {
                    set_phase(&handle, QrPhase::Failed(format!("初始化失败：{error}")));
                    return;
                }
            };

            let png = match login.create() {
                Ok(png) => png,
                Err(error) => {
                    set_phase(&handle, QrPhase::Failed(format!("获取二维码失败：{error}")));
                    return;
                }
            };
            let name = crate::assets::put_image(png);
            if let Ok(mut guard) = handle.lock() {
                guard.image = Some(name);
                guard.phase = Some(QrPhase::Waiting);
            }

            // 轮询：每 2 秒一次，最多 3 分钟
            for _ in 0..90 {
                std::thread::sleep(std::time::Duration::from_secs(2));
                match login.poll() {
                    Ok(Some(redirect)) => {
                        let cookie = match login.exchange_cookie(&redirect) {
                            Ok(cookie) => cookie,
                            Err(error) => {
                                set_phase(&handle, QrPhase::Failed(format!("换取凭证失败：{error}")));
                                return;
                            }
                        };
                        let profile = login.fetch_profile(&cookie).unwrap_or_default();
                        let display = profile
                            .name
                            .clone()
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or_else(|| "新账号".to_string());
                        let class_id = profile.class_id.clone().unwrap_or_default();

                        // 直接写进 data.json（与 Python 版一样：扫码成功即建任务）
                        let mut config =
                            bjmf_core::Config::load(&config_path).unwrap_or_default();
                        config.students.push(bjmf_core::Student {
                            name: display.clone(),
                            class: class_id.clone(),
                            acc: "30".into(),
                            cookie: cookie.clone(),
                            ..Default::default()
                        });
                        if let Err(error) = config.save(&config_path) {
                            set_phase(&handle, QrPhase::Failed(format!("保存账号失败：{error}")));
                            return;
                        }

                        if let Ok(mut guard) = handle.lock() {
                            guard.account = Some((display.clone(), class_id, cookie));
                            guard.phase = Some(QrPhase::Done { name: display });
                        }
                        return;
                    }
                    Ok(None) => continue,
                    Err(_) => continue,
                }
            }
            set_phase(&handle, QrPhase::Failed("二维码已过期，请重新生成".into()));
        })
        .ok();

    shared
}

fn set_phase(handle: &QrHandle, phase: QrPhase) {
    if let Ok(mut guard) = handle.lock() {
        guard.phase = Some(phase);
    }
}

/// 把后台进度同步进界面状态；返回 true 表示还在进行中。
pub fn poll(view: &mut AppView) -> bool {
    let Some(handle) = view.qr.clone() else {
        return false;
    };
    let (phase, image, account) = match handle.lock() {
        Ok(guard) => (guard.phase.clone(), guard.image.clone(), guard.account.clone()),
        Err(_) => return false,
    };

    if let Some(image) = image {
        if view.qr_image.as_deref() != Some(image.as_str()) {
            view.qr_image = Some(image);
        }
    }

    match phase {
        Some(QrPhase::Done { name }) => {
            // 重新载入配置，让新账号出现在侧栏
            view.state = crate::state::AppState::load(view.state.paths.clone());
            view.state.running = true;
            view.state
                .log(LogKind::Ok, format!("扫码成功，已添加账号 {name}"));
            if let Some((name, class, cookie)) = account {
                let student = view.state.students().last().cloned();
                if let Some(student) = student {
                    view.state.select(view.state.students().len() - 1);
                    let _ = (name, class, cookie, student);
                }
            }
            view.qr = None;
            view.qr_image = None;
            false
        }
        Some(QrPhase::Failed(message)) => {
            view.state.log(LogKind::Bad, message.clone());
            view.qr_error = Some(message);
            view.qr = None;
            false
        }
        _ => true,
    }
}

/// 扫码弹窗的内容视图。
///
/// 内容（二维码图、状态行）跟着后台线程变，所以必须是一个会自己刷新的实体：
/// 它观察 `AppView`，主界面每轮轮询更新状态时它会重绘。
pub struct QrModal {
    owner: WeakEntity<AppView>,
}

impl QrModal {
    /// 打开弹窗；二维码的拉取由 `AppView::start_qr_session` 负责。
    pub fn open(owner: WeakEntity<AppView>, window: &mut Window, cx: &mut Context<AppView>) {
        let content = cx.new(|cx| {
            if let Some(app) = owner.upgrade() {
                // 主界面 notify（轮询到新状态）时跟着重绘
                cx.observe(&app, |_, _, cx| cx.notify()).detach();
            }
            QrModal {
                owner: owner.clone(),
            }
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("添加账号")
                .w(px(420.0))
                .centered()
                .child(content.clone())
        });
    }
}

impl Render for QrModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (dark, image, error) = {
            let app = owner.read(cx);
            (app.dark, app.qr_image.clone(), app.qr_error.clone())
        };
        let p: &'static Palette = if dark { &DARK } else { &LIGHT };

        let status_text = if let Some(error) = &error {
            error.clone()
        } else if image.is_some() {
            "等待扫码…".to_string()
        } else {
            "正在获取二维码…".to_string()
        };
        let dot_color = if error.is_some() { p.bad } else { p.warn };

        let qr_area = match &image {
            Some(path) => div()
                .w(px(196.0))
                .h(px(196.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .bg(rgb(0xffffff))
                .border_1()
                .border_color(rgb(p.line))
                .child(img(SharedString::from(path.clone())).w(px(184.0)).h(px(184.0)))
                .into_any_element(),
            None => div()
                .w(px(196.0))
                .h(px(196.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .bg(rgb(p.surface_2))
                .border_1()
                .border_color(rgb(p.line_2))
                .text_size(px(11.5))
                .text_color(rgb(p.text_3))
                .child("二维码加载中")
                .into_any_element(),
        };

        let cancel = {
            let owner = owner.clone();
            Button::new("qr-cancel")
                .small()
                .label("取消")
                .on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |view, cx| view.cancel_qr(cx));
                    window.close_dialog(cx);
                })
        };
        let retry = {
            let owner = owner.clone();
            Button::new("qr-retry")
                .small()
                .primary()
                .label("重新生成")
                .on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |view, cx| view.restart_qr(cx));
                })
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(11.0))
            .child(qr_area)
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(p.text))
                    .child("用微信扫码并确认"),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_3))
                    .child("扫码成功后自动创建任务"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(11.0))
                    .py(px(9.0))
                    .rounded(px(8.0))
                    .bg(rgb(p.surface_2))
                    .border_1()
                    .border_color(rgb(p.line))
                    .child(dot(dot_color, false, p))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_2))
                            .child(status_text),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_3))
                    .text_align(gpui_kit::TextAlign::Center)
                    .child("扫码完成后可继续添加账号，或直接在主界面填入地点与时间。"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pt(px(4.0))
                    .child(div().flex_1())
                    .child(cancel)
                    .child(retry),
            )
            .into_any_element()
    }
}
