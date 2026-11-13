//! 「已最小化到托盘」提示。
//!
//! 原来画在主窗口里的覆盖层，问题是它跟着主窗口一起隐藏 —— 用户根本看不到，
//! 等于这个提示不存在。改成独立的小浮窗：贴屏幕右下角、置顶、不抢焦点。

use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{
    div, point, px, rgb, size, AppContext, Bounds, Context, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, WindowBounds, WindowKind,
    WindowOptions,
};

use crate::system::win;
use crate::theme::{DARK, LIGHT};
use crate::ui::widgets::{button, BtnKind, BtnSize};

/// 「不再提示」：本次运行内有效（不落盘，避免为一个提示项加设置字段）。
static MUTED: AtomicBool = AtomicBool::new(false);

pub fn is_muted() -> bool {
    MUTED.load(Ordering::Relaxed)
}

pub struct CloseHintView {
    dark: bool,
}

impl Render for CloseHintView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = if self.dark { &DARK } else { &LIGHT };

        div()
            .size_full()
            .flex()
            .gap(px(10.0))
            .p(px(12.0))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line_2))
            .font_family("Microsoft YaHei UI")
            .child(
                div()
                    .flex_none()
                    .w(px(18.0))
                    .h(px(18.0))
                    .mt(px(1.0))
                    .child(gpui_kit::img("logo-64.png").w(px(18.0)).h(px(18.0))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(p.text))
                            .child("已最小化到托盘"),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child("定时签到仍在后台运行，要退出请右键托盘图标"),
                    )
                    .child(
                        div()
                            .mt(px(9.0))
                            .flex()
                            .gap(px(6.0))
                            .child(
                                button("打开主窗口", BtnKind::Primary, BtnSize::Xs, false, p)
                                    .id("hint-open-window")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|_, _, window, _| {
                                        win::show_main_window();
                                        window.remove_window();
                                    })),
                            )
                            .child(
                                button("不再提示", BtnKind::Default, BtnSize::Xs, false, p)
                                    .id("hint-mute")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|_, _, window, _| {
                                        MUTED.store(true, Ordering::Relaxed);
                                        window.remove_window();
                                    })),
                            ),
                    ),
            )
    }
}

/// 在屏幕右下角弹出提示（主窗口隐藏之后再调用）。
pub fn open(cx: &mut gpui_kit::App, dark: bool) {
    let (_, _, right, bottom) = win::work_area();
    let w = 340.0_f32;
    let h = 116.0_f32;
    let margin = 16.0_f32;

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(right as f32 - w - margin), px(bottom as f32 - h - margin)),
            size: size(px(w), px(h)),
        })),
        titlebar: None,
        focus: false,
        show: true,
        // PopUp = 置顶但不抢焦点，正好适合这种一次性提示
        kind: WindowKind::PopUp,
        is_movable: false,
        ..Default::default()
    };

    let _ = cx.open_window(options, move |_, cx| cx.new(|_| CloseHintView { dark }));
}
