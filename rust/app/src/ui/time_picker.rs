//! 「添加执行时间」：快捷时间 + 时/分下拉点选，不再手输 `21:31:00`。
//!
//! 分钟粒度是 5 分钟 —— 签到时间点没必要精确到个位，60 个格子也点得累。
//!
//! 结构：内容直接写在组件库 Dialog 的 builder 里（和「编辑密钥」弹窗一致）。
//! 之所以不像扫码弹窗那样做成 `Entity<视图>`：这里唯一会变的是两个下拉的选中值，
//! 而 `Select` 本身就是实体、选中后自己重绘，不需要外层再包一个视图。
//! 于是：
//! - 快捷项是一次性动作（点一下直接添加并关闭），不存「选中高亮」这种状态；
//! - 冲突/失败用库的 `AlertDialog` 说明原因，不在内容里留错误行；
//! - 点「添加」时现读两个下拉的选中值（`selected_value`），不维护影子状态。

use chrono::Timelike;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _};
use gpui_kit::component::IndexPath;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Entity, SharedString, WeakEntity, Window};

use crate::ui::main_view::{select_box, AppView};

const PANEL_W: f32 = 420.0;
/// 分钟粒度：0, 5, 10 ... 55
pub const MINUTE_STEP: u32 = 5;

/// 快捷时间：早上上学、午间、晚上各一个。
const QUICK: [(u32, u32); 3] = [(7, 30), (12, 0), (21, 30)];

type NumberSelect = Entity<SelectState<Vec<SharedString>>>;

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

/// 生成下拉选项：`00`..`23` / `00`, `05` … `55`。
fn labels(from: u32, to: u32, step: u32) -> Vec<SharedString> {
    let mut out = Vec::new();
    let mut value = from;
    while value <= to {
        out.push(SharedString::from(format!("{value:02}")));
        value += step.max(1);
    }
    out
}

/// 从下拉实体里读当前选中的数值。
fn selected_number(state: &NumberSelect, cx: &App) -> u32 {
    state
        .read(cx)
        .selected_value()
        .and_then(|value| value.to_string().parse::<u32>().ok())
        .unwrap_or(0)
}

/// 写入一个时间点：成功就关弹窗，失败用 AlertDialog 说明原因。
fn add(owner: WeakEntity<AppView>, hour: u32, minute: u32, window: &mut Window, cx: &mut App) {
    let value = to_config_value(hour, minute);
    let Some(view) = owner.upgrade() else {
        window.close_dialog(cx);
        return;
    };
    match view.update(cx, |view, cx| view.add_time_from_picker(&value, cx)) {
        Ok(()) => window.close_dialog(cx),
        Err(error) => {
            window.close_dialog(cx);
            let title = format!("{} 没能添加", pick_text(hour, minute));
            window.open_alert_dialog(cx, move |alert, _, _| {
                alert
                    .title(title.clone())
                    .description(error.clone())
                    .ok_variant(ButtonVariant::Danger)
            });
        }
    }
}

/// 打开「添加执行时间」弹窗。
pub fn open(
    owner: WeakEntity<AppView>,
    times: &[String],
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let (hour, minute) = default_pick(times);

    // 两个下拉的实体必须提前建好：dialog 的 builder 可能重跑，不能在里面 new。
    let hour_select: NumberSelect = cx.new(|cx| {
        SelectState::new(
            labels(0, 23, 1),
            Some(IndexPath::new(hour as usize)),
            window,
            cx,
        )
    });
    let minute_select: NumberSelect = cx.new(|cx| {
        SelectState::new(
            labels(0, 55, MINUTE_STEP),
            Some(IndexPath::new((minute / MINUTE_STEP) as usize)),
            window,
            cx,
        )
    });

    window.open_dialog(cx, move |dialog, _window, cx| {
        let hint_color = cx.theme().muted_foreground;

        // ---- 快捷：点一下直接添加（少两步点击）----
        let mut quick = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for (h, m) in QUICK {
            let owner = owner.clone();
            quick = quick.child(
                Button::new(SharedString::from(format!("quick-{h}-{m}")))
                    .xsmall()
                    .label(pick_text(h, m))
                    .on_click(move |_, window, cx| add(owner.clone(), h, m, window, cx)),
            );
        }
        let owner_now = owner.clone();
        quick = quick.child(
            Button::new("quick-now")
                .xsmall()
                .label("当前时间")
                .on_click(move |_, window, cx| {
                    let (h, m) = now_pick();
                    add(owner_now.clone(), h, m, window, cx);
                }),
        );

        // ---- 时 / 分（库的 Select，选中值自己维护）----
        let confirm = {
            let owner = owner.clone();
            let hour_select = hour_select.clone();
            let minute_select = minute_select.clone();
            Button::new("picker-confirm")
                .small()
                .primary()
                .label("添加")
                .on_click(move |_, window, cx| {
                    let hour = selected_number(&hour_select, cx);
                    let minute = selected_number(&minute_select, cx);
                    add(owner.clone(), hour, minute, window, cx);
                })
        };
        let cancel = Button::new("picker-cancel")
            .small()
            .label("取消")
            .on_click(|_, window, cx| window.close_dialog(cx));

        dialog
            .title("添加执行时间")
            .w(px(PANEL_W))
            .centered()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(div().text_size(px(10.5)).text_color(hint_color).child("快捷"))
                    .child(quick)
                    .child(div().text_size(px(10.5)).text_color(hint_color).child("时间"))
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .gap(px(8.0))
                            .child(select_box(70.0, Select::new(&hour_select)))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(hint_color)
                                    .child(":"),
                            )
                            .child(select_box(70.0, Select::new(&minute_select)))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(hint_color)
                                    .child("5 分钟一档"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(hint_color)
                                    .child("点「时」或「分」展开列表选择"),
                            )
                            .child(div().flex_1())
                            .child(cancel)
                            .child(confirm),
                    ),
            )
    });
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

    #[test]
    fn 小时与分钟选项数量与补零都正确() {
        assert_eq!(labels(0, 23, 1).len(), 24);
        assert_eq!(labels(0, 55, MINUTE_STEP).len(), 12);
        assert_eq!(labels(0, 23, 1)[0].to_string(), "00");
        assert_eq!(labels(0, 55, MINUTE_STEP)[1].to_string(), "05");
    }
}
