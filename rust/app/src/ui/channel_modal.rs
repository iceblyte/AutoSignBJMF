//! 通知渠道弹窗：编辑密钥 / 查看密钥。
//!
//! 主界面上不再直接摊开 Key 输入框 —— 渠道关掉时整块收起，打开后右侧给
//! 「编辑」「查看」两个入口，各自开一个弹窗：
//!
//! - **编辑**：真的能改，保存时写回 `data.json`（只动这一个字段）
//! - **查看**：打码显示（`SCT••••••4f2a`），带一个「复制」
//!
//! 版式沿用时间选择器那套：整窗遮罩 + 居中卡片 + 标题栏 + 底部按钮行。
//! 注意遮罩**不挂点击关闭**：遮罩的 mouse_down 会抢在按钮的 click 之前触发，
//! 点「保存」就会变成「先关弹窗再丢点击」。

use gpui_kit::prelude::*;
use gpui_kit::{div, px, rgba, rgb, AnyElement, Context, Div};

use crate::state::mask_key;
use crate::theme::*;
use crate::ui::main_view::{channel_field_label, channel_hint, channel_name, AppView, Overlay};
use crate::ui::widgets::{button, BtnKind, BtnSize};

const PANEL_W: f32 = 470.0;

fn header(title: String, p: &Palette, cx: &mut Context<AppView>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(15.0))
        .py(px(13.0))
        .border_b_1()
        .border_color(rgb(p.line))
        .child(
            div()
                .flex_1()
                .text_size(px(13.0))
                .text_color(rgb(p.text))
                .child(title),
        )
        .child(
            div()
                .id("channel-close")
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .text_size(px(14.0))
                .text_color(rgb(p.text_3))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(p.surface_3)))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.overlay = Overlay::None;
                    this.active = None;
                    cx.notify();
                }))
                .child("×"),
        )
}

pub fn render(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let field = match view.overlay {
        Overlay::ChannelEdit(field) | Overlay::ChannelView(field) => field,
        _ => return div().into_any_element(),
    };
    let editing = matches!(view.overlay, Overlay::ChannelEdit(_));
    let name = channel_name(field);
    let raw = view.channel_key(field);
    let empty = raw.trim().is_empty();

    let title = if editing {
        format!("编辑{name}密钥")
    } else {
        format!("查看{name}密钥")
    };

    let body = if editing {
        let hint = if empty {
            format!("{}（当前为空）", channel_hint(field))
        } else {
            format!("{}，留空表示清除", channel_hint(field))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(p.text_3))
                    .child(hint),
            )
            .child(view.text_field(field, true, false, p, cx))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(p.text_3))
                    .child("密钥只保存在本机 data.json，不会上传到任何地方"),
            )
    } else {
        let shown = if empty {
            "未填写".to_string()
        } else {
            mask_key(&raw)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .h(px(34.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .px(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgb(p.surface_2))
                    .border_1()
                    .border_color(rgb(p.line_2))
                    .text_size(px(12.5))
                    .text_color(rgb(if empty { p.text_3 } else { p.text }))
                    .child(shown),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(p.text_3))
                    .child(format!(
                        "完整密钥存在 data.json 的 {} 字段里",
                        channel_field_label(field)
                    )),
            )
    };

    let footer_left = if editing {
        div()
            .text_size(px(11.0))
            .text_color(rgb(p.text_3))
            .child("保存后立即生效")
    } else {
        div()
            .text_size(px(11.0))
            .text_color(rgb(p.text_3))
            .child(if empty { "还没有配置密钥" } else { "只显示首三尾四" })
    };

    let mut actions = div().flex().items_center().gap(px(8.0));
    if editing {
        actions = actions
            .child(
                button("取消", BtnKind::Default, BtnSize::Small, false, p)
                    .id("channel-cancel")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel_channel_edit(cx);
                    })),
            )
            .child(
                button("保存", BtnKind::Primary, BtnSize::Small, false, p)
                    .id("channel-save")
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.save_channel_key(field, cx);
                    })),
            );
    } else {
        if !empty {
            actions = actions.child(
                button("复制", BtnKind::Default, BtnSize::Small, false, p)
                    .id("channel-copy")
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let text = this.channel_key(field);
                        match crate::ui::main_view::copy_to_clipboard(&text) {
                            Ok(()) => this.state.log(
                                crate::state::LogKind::Info,
                                format!("{}密钥已复制到剪贴板", channel_name(field)),
                            ),
                            Err(_) => this.state.log(
                                crate::state::LogKind::Bad,
                                "复制失败：剪贴板被其它程序占用".to_string(),
                            ),
                        }
                        cx.notify();
                    })),
            );
        }
        actions = actions.child(
            button("关闭", BtnKind::Primary, BtnSize::Small, false, p)
                .id("channel-done")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.overlay = Overlay::None;
                    this.active = None;
                    cx.notify();
                })),
        );
    }

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0f121675))
        // 遮罩必须吃掉鼠标：否则点在弹窗外壳上会穿透到后面的主界面控件
        // （时间选择器里点「时」会同时点到底下「失败重试」的下拉，就是这个原因）
        .occlude()
        .child(
            div()
                .w(px(PANEL_W))
                .flex()
                .flex_col()
                .rounded(px(14.0))
                .bg(rgb(p.surface))
                .border_1()
                .border_color(rgb(p.line))
                .overflow_hidden()
                .child(header(title, p, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .px(px(16.0))
                        .py(px(14.0))
                        .child(body),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(16.0))
                        .py(px(13.0))
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .child(footer_left)
                        .child(div().flex_1())
                        .child(actions),
                ),
        )
        .into_any_element()
}
