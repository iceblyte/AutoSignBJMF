//! 通知渠道密钥弹窗：改用组件库的 Dialog。
//!
//! 原来是自己画遮罩 + 居中卡片 + 标题栏 + 按钮行（约 200 行），现在只描述内容：
//! 库的 Dialog 负责遮罩、居中、Esc 关闭、点遮罩关闭、焦点与入场动画。
//!
//! 两条使用库 Dialog 的注意事项（都写在库文档里，踩过才知道）：
//! 1. builder **可能重跑**，所以持久状态（InputState）必须提前建好，
//!    在 builder 里只读；否则每重建一次就把用户输入丢了。
//! 2. 关闭要显式调 `window.close_dialog(cx)`。

use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Window};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _};

use crate::state::mask_key;
use crate::ui::main_view::{channel_field_label, channel_hint, channel_name, AppView, Field};

const PANEL_W: f32 = 470.0;

/// 打开「编辑密钥」弹窗。
pub fn open(view: &mut AppView, field: Field, window: &mut Window, cx: &mut Context<AppView>) {
    // 草稿按磁盘内容重灌，并把值推回输入框（版本号 +1，渲染时同步）
    view.state.reload_draft();
    view.sync_inputs(window, cx);

    let this = cx.entity();
    let title = format!("编辑{}密钥", channel_name(field));

    window.open_dialog(cx, move |dialog, _window, cx| {
        let app = this.read(cx);
        let raw = app.channel_key(field);
        let empty = raw.trim().is_empty();
        let hint = if empty {
            format!("{}（当前为空）", channel_hint(field))
        } else {
            format!("{}，留空表示清除", channel_hint(field))
        };
        let field_note = format!(
            "密钥只保存在本机 data.json 的 {} 字段里，不会上传",
            channel_field_label(field)
        );
        let current = if empty {
            "还没有配置密钥，填写后保存才会推送".to_string()
        } else {
            format!("当前：{}", mask_key(&raw))
        };

        let cancel = {
            let this = this.clone();
            Button::new("channel-cancel")
                .label("取消")
                .on_click(move |_, window, cx| {
                    this.update(cx, |view, cx| view.cancel_channel_edit(cx));
                    window.close_dialog(cx);
                })
        };
        let save = {
            let this = this.clone();
            Button::new("channel-save")
                .primary()
                .label("保存")
                .on_click(move |_, window, cx| {
                    this.update(cx, |view, cx| view.save_channel_key(field, cx));
                    window.close_dialog(cx);
                })
        };

        let state = match field {
            Field::Qq => app.qq_input.clone(),
            _ => app.wx_input.clone(),
        };

        dialog
            .title(title.clone())
            .w(px(PANEL_W))
            .centered()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(note(hint, cx))
                    .child(Input::new(&state))
                    .child(note(field_note, cx))
                    .child(note(current, cx)),
            )
            .footer(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(cancel)
                    .child(save),
            )
    });
}

/// 说明文字：跟随主题的次要前景色。
fn note(text: String, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(11.0))
        .text_color(cx.theme().muted_foreground)
        .child(text)
}
