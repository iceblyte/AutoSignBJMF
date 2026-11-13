//! 「添加执行时间」：快捷时间 + 时/分下拉点选，不再手输 `21:31:00`。
//!
//! 分钟粒度是 5 分钟 —— 签到时间点没必要精确到个位，60 个格子也点得累。
//! 布局沿用原型的组件语言：14px 圆角卡片 + 顶部标题栏 + 底部按钮行。

use chrono::Timelike;
use gpui_kit::base::StyledExt;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, rgba, rgb, AnyElement, ClickEvent, Context, SharedString};

use crate::theme::*;
use crate::ui::main_view::{AppView, OpenSelect, Overlay};
use crate::ui::widgets::{button, choice_chip, BtnKind, BtnSize};

const PANEL_W: f32 = 420.0;
/// 分钟粒度：0, 5, 10 ... 55
const MINUTE_STEP: u32 = 5;

/// 快捷时间：早上上学、午间、晚上各一个。
const QUICK: [(u32, u32); 3] = [(7, 30), (12, 0), (21, 30)];

/// 打开选择器时的默认时间：取列表里最后一个，往后挪一小时并对齐到 5 分钟；没有就 07:30。
pub fn default_pick(times: &[String]) -> (u32, u32) {
    let Some(last) = times.last() else {
        return (7, 30);
    };
    let mut parts = last.split(':');
    let hour: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(7);
    let minute: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(30);
    ((hour + 1) % 24, round_minute(minute))
}

/// 现在的时间，分钟向下取整到 5 分钟。
pub fn now_pick() -> (u32, u32) {
    let now = chrono::Local::now();
    (now.hour(), round_minute(now.minute()))
}

fn round_minute(minute: u32) -> u32 {
    (minute / MINUTE_STEP) * MINUTE_STEP
}

/// 把关掉秒后的 `HH:MM` 拼成配置里用的 `HH:MM:SS`。
pub fn to_config_value(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}:00")
}

fn pick_text(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

/// 一个下拉触发器（比输入框矮一点，和列表块同高）。
fn dropdown(
    value: String,
    open: bool,
    width: f32,
    id: &'static str,
    p: &Palette,
    cx: &mut Context<AppView>,
    target: OpenSelect,
) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .w(px(width))
        .h(px(32.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.0))
        .px(px(9.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgb(if open { p.accent } else { p.line_2 }))
        .bg(rgb(p.surface))
        .child(
            div()
                .flex_1()
                .text_size(px(14.0))
                .text_color(rgb(p.text))
                .child(value),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(p.text_3))
                .child(if open { "▲" } else { "▼" }),
        )
        .cursor_pointer()
        .hover(|style| style.border_color(rgb(p.text_3)))
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
            if this.open_select == target {
                this.open_select = OpenSelect::None;
                this.dropdown_at = None;
            } else {
                this.open_select = target;
                let pos = event.position();
                this.dropdown_at = Some((target, pos.x, pos.y));
            }
            cx.notify();
        }))
        .into_any_element()
}

pub fn render(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let (hour, minute) = (view.picker_hour, view.picker_minute);
    let open_hour = view.open_select == OpenSelect::Hour;
    let open_minute = view.open_select == OpenSelect::Minute;

    // ---- 快捷时间：常用时刻 + 当前时间 ----
    let mut quick = div().flex().items_center().gap(px(4.0)).flex_wrap();
    for (h, m) in QUICK {
        let selected = h == hour && m == minute;
        let chip = choice_chip(pick_text(h, m), selected, 58.0, p);
        quick = quick.child(if selected {
            chip.into_any_element()
        } else {
            chip.id(SharedString::from(format!("quick-{h}-{m}")))
                .on_click(cx.listener(move |this, _, _, cx| {
                this.picker_hour = h;
                this.picker_minute = m;
                this.open_select = OpenSelect::None;
                cx.notify();
            }))
            .into_any_element()
        });
    }
    let now = now_pick();
    let now_selected = now == (hour, minute);
    let now_chip = choice_chip("当前时间", now_selected, 74.0, p);
    quick = quick.child(if now_selected {
        now_chip.into_any_element()
    } else {
        now_chip
            .id("quick-now")
            .on_click(cx.listener(|this, _, _, cx| {
                let (h, m) = now_pick();
                this.picker_hour = h;
                this.picker_minute = m;
                this.open_select = OpenSelect::None;
                cx.notify();
            }))
            .into_any_element()
    });

    // ---- 时 / 分下拉 ----
    let hour_dd = dropdown(
        format!("{hour:02}"),
        open_hour,
        70.0,
        "pick-hour",
        p,
        cx,
        OpenSelect::Hour,
    );
    let minute_dd = dropdown(
        format!("{minute:02}"),
        open_minute,
        70.0,
        "pick-minute",
        p,
        cx,
        OpenSelect::Minute,
    );

    // 时 / 分一行；展开的列表另起一行（否则会被 flex_row 挤到右边）
    let mut picker_block = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .child(hour_dd)
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(p.text_3))
                        .child(":"),
                )
                .child(minute_dd)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(p.text_3))
                        .child("5 分钟一档"),
                ),
        );

    let conflict = view
        .state
        .settings
        .times
        .iter()
        .any(|existing| existing == &to_config_value(hour, minute));

    let hint = if conflict {
        div()
            .text_size(px(11.0))
            .text_color(rgb(p.warn))
            .child(format!("{} 已经在列表里了", pick_text(hour, minute)))
    } else {
        div()
            .text_size(px(11.0))
            .text_color(rgb(p.text_3))
            .child("点「时」或「分」展开列表选择")
    };

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
                                    this.open_select = OpenSelect::None;
                                    cx.notify();
                                }))
                                .child("×"),
                        ),
                )
                // ---- 内容 ----
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .px(px(16.0))
                        .py(px(14.0))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(p.text_3))
                                .child("快捷"),
                        )
                        .child(quick)
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(p.text_3))
                                .child("时间"),
                        )
                        .child(picker_block),
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
                                    this.open_select = OpenSelect::None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            button("添加", BtnKind::Primary, BtnSize::Small, conflict, p)
                                .id("picker-confirm")
                                .when(!conflict, |element| {
                                    element.cursor_pointer().on_click(cx.listener(
                                        |this, _, _, cx| {
                                            let value = to_config_value(
                                                this.picker_hour,
                                                this.picker_minute,
                                            );
                                            match this.state.add_time(&value) {
                                                Ok(()) => {
                                                    this.overlay = Overlay::None;
                                                    this.open_select = OpenSelect::None;
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
    fn 默认值是列表最后一个时间加一小时并对齐到五分钟() {
        let times = vec!["07:30:00".to_string(), "21:31:00".to_string()];
        // 21:31 + 1h = 22:31，分钟对齐到 30
        assert_eq!(default_pick(&times), (22, 30));
    }

    #[test]
    fn 二十三点加一小时回到零点() {
        let times = vec!["23:05:00".to_string()];
        assert_eq!(default_pick(&times), (0, 5));
    }

    #[test]
    fn 分钟按五取整() {
        assert_eq!(round_minute(0), 0);
        assert_eq!(round_minute(4), 0);
        assert_eq!(round_minute(5), 5);
        assert_eq!(round_minute(31), 30);
        assert_eq!(round_minute(59), 55);
    }

    #[test]
    fn 当前时间总是落在五分钟档位上() {
        let (_, minute) = now_pick();
        assert_eq!(minute % MINUTE_STEP, 0, "分钟应当是 5 的倍数：{minute}");
    }

    #[test]
    fn 拼接成配置用的格式() {
        assert_eq!(to_config_value(9, 5), "09:05:00");
        assert_eq!(to_config_value(23, 55), "23:55:00");
    }
}
