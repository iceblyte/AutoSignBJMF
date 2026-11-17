//! 通知渠道密钥弹窗（只有「编辑」一种）。
//!
//! 主界面上不再直接摊开 Key 输入框，也不再有只读的「查看」弹窗：渠道打开时
//! 右侧只有一个「编辑」入口，弹窗里就是明文的输入框 —— 既能看也能改，
//! 少一层点击。关闭渠道时整块收起，只留一句「已关闭，不发送」。
//!
//! 版式沿用时间选择器那套：整窗遮罩 + 居中卡片 + 标题栏 + 底部按钮行。
//! 注意遮罩**不挂点击关闭**：遮罩的 mouse_down 会抢在按钮的 click 之前触发，
//! 点「保存」就会变成「先关弹窗再丢点击」。

use gpui_kit::prelude::*;
use gpui_kit::{div, px, rgba, rgb, AnyElement, Context, Div};

use crate::state::mask_key;
use crate::theme::*;
use crate::ui::main_view::{channel_field_label, channel_hint, channel_name, AppView, Overlay};
use crate::ui::widgets::{button, click, BtnKind, BtnSize};

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
                    this.cancel_channel_edit(cx);
                }))
                .child("×"),
        )
}

pub fn render(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let field = match view.overlay {
        Overlay::ChannelEdit(field) => field,
        _ => return div().into_any_element(),
    };
    let name = channel_name(field);
    let raw = view.channel_key(field);
    let empty = raw.trim().is_empty();

    let hint = if empty {
        format!("{}（当前为空）", channel_hint(field))
    } else {
        format!("{}，留空表示清除", channel_hint(field))
    };

    let body = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(div().text_size(px(11.0)).text_color(rgb(p.text_3)).child(hint))
        .child(view.text_field(field, true, false, p, cx))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(rgb(p.text_3))
                .child(format!(
                    "密钥只保存在本机 data.json 的 {} 字段里，不会上传",
                    channel_field_label(field)
                )),
        )
        .child(
            div()
                .text_size(px(10.5))
                .text_color(rgb(p.text_3))
                .child(if empty {
                    "还没有配置密钥，填写后保存才会推送".to_string()
                } else {
                    format!("当前：{}", mask_key(&raw))
                }),
        );

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0f121675))
        // 遮罩必须吃掉鼠标：否则点在弹窗外壳上会穿透到后面的主界面控件
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
                .child(header(format!("编辑{name}密钥"), p, cx))
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
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(p.text_3))
                                .child("保存后立即生效"),
                        )
                        .child(div().flex_1())
                        .child(
                            button("取消", BtnKind::Default, BtnSize::Small, false, p)
                                .id("channel-cancel")
                                .cursor_pointer()
                                .on_click(click(cx, |this, _, _, cx| {
                                    this.cancel_channel_edit(cx);
                                })),
                        )
                        .child(
                            button("保存", BtnKind::Primary, BtnSize::Small, false, p)
                                .id("channel-save")
                                .cursor_pointer()
                                .on_click(click(cx, move |this, _, _, cx| {
                                    this.save_channel_key(field, cx);
                                })),
                        ),
                ),
        )
        .into_any_element()
}
