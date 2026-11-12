//! 「添加执行时间」：从列表里点选小时与分钟，不再手输 `21:31:00` 这种字符串。
//!
//! 布局沿用原型的组件语言：14px 圆角卡片 + 顶部标题栏 + 底部按钮行；
//! 选中态用强调色实底，未选中是 `surface-2` 描边块，悬停提亮一档。

use gpui_kit::base::StyledExt;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, rgba, rgb, AnyElement, Context, SharedString};

use crate::theme::*;
use crate::ui::main_view::{AppView, Overlay};
use crate::ui::widgets::{button, BtnKind, BtnSize};

const PANEL_W: f32 = 428.0;

/// 打开选择器时的默认时间：取列表里最后一个，往后挪一小时；没有就 07:30。
///
/// 纯函数，便于单测。
pub fn default_pick(times: &[String]) -> (u32, u32) {
    let Some(last) = times.last() else {
        return (7, 30);
    };
    let mut parts = last.split(':');
    let hour: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(7);
    let minute: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(30);
    ((hour + 1) % 24, minute)
}

/// 把关掉秒后的 `HH:MM` 拼成配置里用的 `HH:MM:SS`。
pub fn to_config_value(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}:00")
}

/// 一个可点的小格子。
fn pick_cell(
    label: String,
    selected: bool,
    width: f32,
    id: &str,
    p: &Palette,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView) + 'static,
) -> AnyElement {
    let base = div()
        .id(SharedString::from(id.to_string()))
        .w(px(width))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .text_size(px(11.5))
        .child(label);

    if selected {
        base.bg(rgb(p.accent))
            .border_color(rgb(p.accent))
            .text_color(rgb(0xffffff))
            .into_any_element()
    } else {
        base.bg(rgb(p.surface_2))
            .border_color(rgb(p.line))
            .text_color(rgb(p.text_2))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(p.surface_3)).text_color(rgb(p.text)))
            .on_click(cx.listener(move |this, _, _, cx| {
                on_pick(this);
                cx.notify();
            }))
            .into_any_element()
    }
}

pub fn render(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let picked = (view.picker_hour, view.picker_minute);
    let preview = format!("{:02}:{:02}", picked.0, picked.1);

    // ---- 小时：6 列 × 4 行 ----
    let mut hours = div().flex().flex_wrap().gap(px(4.0)).w_full();
    for hour in 0..24u32 {
        hours = hours.child(pick_cell(
            format!("{hour:02}"),
            hour == picked.0,
            60.0,
            &format!("pick-h-{hour}"),
            p,
            cx,
            move |this| this.picker_hour = hour,
        ));
    }

    // ---- 分钟：10 列 × 6 行 ----
    let mut minutes = div().flex().flex_wrap().gap(px(3.0)).w_full();
    for minute in 0..60u32 {
        minutes = minutes.child(pick_cell(
            format!("{minute:02}"),
            minute == picked.1,
            36.0,
            &format!("pick-m-{minute}"),
            p,
            cx,
            move |this| this.picker_minute = minute,
        ));
    }

    let conflict = view
        .state
        .settings
        .times
        .iter()
        .any(|existing| existing == &to_config_value(picked.0, picked.1));

    let mut hint = div()
        .text_size(px(11.0))
        .text_color(rgb(p.text_3))
        .child("选好小时后点分钟；已经是列表里的时间会被拦下");

    if conflict {
        hint = div()
            .text_size(px(11.0))
            .text_color(rgb(p.warn))
            .child(format!("{preview} 已经在列表里了"));
    }

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0f121675))
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
                // ---- 标题栏 ----
                .child(
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
                                .text_size(px(13.0))
                                .text_color(rgb(p.text))
                                .child("添加执行时间"),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id("picker-close")
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
                                    cx.notify();
                                }))
                                .child("×"),
                        ),
                )
                // ---- 预览 ----
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(10.0))
                        .px(px(16.0))
                        .pt(px(14.0))
                        .child(
                            div()
                                .text_size(px(30.0))
                                .text_color(rgb(p.text))
                                .child(preview),
                        )
                        .child(
                            div()
                                .pb(px(5.0))
                                .text_size(px(11.0))
                                .text_color(rgb(p.text_3))
                                .child("时 : 分（秒固定 00）"),
                        ),
                )
                // ---- 小时 ----
                .child(
                    div()
                        .px(px(16.0))
                        .pt(px(12.0))
                        .pb(px(4.0))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(p.text_3))
                                .child("小时"),
                        )
                        .child(hours),
                )
                // ---- 分钟 ----
                .child(
                    div()
                        .px(px(16.0))
                        .pt(px(10.0))
                        .pb(px(4.0))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(p.text_3))
                                .child("分钟"),
                        )
                        .child(minutes),
                )
                // ---- 底部 ----
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(16.0))
                        .py(px(13.0))
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .child(hint)
                        .child(div().flex_1())
                        .child(
                            button("取消", BtnKind::Default, BtnSize::Small, false, p)
                                .id("picker-cancel")
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.overlay = Overlay::None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            button("添加", BtnKind::Primary, BtnSize::Small, conflict, p)
                                .id("picker-confirm")
                                .when(!conflict, |element| {
                                    element.cursor_pointer().on_click(cx.listener(
                                        |this, _, _, cx| {
                                            let value =
                                                to_config_value(this.picker_hour, this.picker_minute);
                                            match this.state.add_time(&value) {
                                                Ok(()) => {
                                                    this.overlay = Overlay::None;
                                                    this.error = None;
                                                }
                                                Err(error) => this.error = Some(error.0),
                                            }
                                            cx.notify();
                                        },
                                    ))
                                }),
                        ),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 没有时间点时默认七点半() {
        assert_eq!(default_pick(&[]), (7, 30));
    }

    #[test]
    fn 默认值是列表最后一个时间加一小时() {
        let times = vec!["07:30:00".to_string(), "21:31:00".to_string()];
        assert_eq!(default_pick(&times), (22, 31));
    }

    #[test]
    fn 二十三点加一小时回到零点() {
        let times = vec!["23:05:00".to_string()];
        assert_eq!(default_pick(&times), (0, 5));
    }

    #[test]
    fn 拼接成配置用的格式() {
        assert_eq!(to_config_value(9, 5), "09:05:00");
        assert_eq!(to_config_value(23, 59), "23:59:00");
    }
}
