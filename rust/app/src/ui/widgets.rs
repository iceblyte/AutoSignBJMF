//! 基础控件：与 `design/v2/proto.css` 的组件一一对应。
//!
//! 每条规则后面都标了原型里的类名与尺寸，方便对照验收；改样式请先改原型。

use gpui_kit::prelude::*;
// 只取需要的类型：`use gpui_kit::*` 会把 gpui 的 `test` 宏一起带进来，
// 导致本文件的 `#[test]` 解析成 gpui 的测试宏而递归展开。
use gpui_kit::{div, px, rgb, AnyElement, Div, SharedString};

use crate::theme::Palette;

/// 文字颜色 + 字号的快捷组合。
pub fn label(text: impl Into<SharedString>, size: f32, color: u32) -> Div {
    div()
        .text_size(px(size))
        .text_color(rgb(color))
        .child(text.into())
}

/// 状态点 `.dot`：7px，ok/warn/bad 带 3px 光晕。
pub fn dot(color: u32, glow: bool, p: &Palette) -> Div {
    let base = div().w(px(7.0)).h(px(7.0)).rounded_full().bg(rgb(color)).flex_none();
    if glow {
        // gpui 没有 box-shadow，用外描边近似那圈 3px 光晕
        base.border_2().border_color(rgb(mix(color, p.surface, 0.86)))
    } else {
        base
    }
}

/// 状态 pill：高 23，圆角 999，11.5px/550，可选前置圆点。
pub fn pill(text: impl Into<SharedString>, fg: u32, bg: u32, border: u32, with_dot: bool) -> Div {
    let base = div()
        .h(px(23.0))
        .px(px(9.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded_full()
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(border))
        .text_size(px(11.5))
        .text_color(rgb(fg))
        .child(div().text_size(px(11.5)).text_color(rgb(fg)).child(text.into()));
    if with_dot {
        base.child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(rgb(fg)).flex_none())
    } else {
        base
    }
}

/// 开关 `.sw`：34×20，选中时强调色，小圆点 16。
pub fn switch(checked: bool, disabled: bool, p: &Palette) -> Div {
    let track = if checked { p.accent } else { p.line_2 };
    let knob_left = if checked { 34.0 - 2.0 - 16.0 } else { 2.0 };
    div()
        .w(px(34.0))
        .h(px(20.0))
        .rounded_full()
        .bg(rgb(track))
        .relative()
        .when(disabled, |d| d.opacity(0.45))
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .left(px(knob_left))
                .w(px(16.0))
                .h(px(16.0))
                .rounded_full()
                .bg(rgb(0xffffff)),
        )
}

/// 选项块：下拉列表 / 时间选择器里的一个可选项。
///
/// 选中态是强调色实底，未选中是 `surface-2` 描边，悬停提亮一档 ——
/// 与主界面的 chip 保持同一套语言。需要点击时由调用方补 `.id(...)`。
pub fn choice_chip(
    label: impl Into<SharedString>,
    selected: bool,
    width: f32,
    p: &Palette,
) -> Div {
    let base = div()
        .h(px(26.0))
        .w(px(width))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .text_size(px(11.5))
        .child(label.into());

    if selected {
        base.bg(rgb(p.accent))
            .border_color(rgb(p.accent))
            .text_color(rgb(0xffffff))
    } else {
        base.bg(rgb(p.surface_2))
            .border_color(rgb(p.line))
            .text_color(rgb(p.text_2))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(p.surface_3)).text_color(rgb(p.text)))
    }
}

/// 按钮的语气。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {    /// 默认：白底 + 描边
    Default,
    /// 主按钮 `.btn-p`：强调色底
    Primary,
    /// 幽灵 `.btn-g`：无描边
    Ghost,
    /// 危险 `.btn-danger`：红字
    Danger,
}

/// 按钮尺寸。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtnSize {
    /// 32px（exe 端主尺寸）
    Normal,
    /// 28px `.btn-sm`
    Small,
    /// 24px `.btn-xs`
    Xs,
}

impl BtnSize {
    fn height(self) -> f32 {
        match self {
            BtnSize::Normal => 32.0,
            BtnSize::Small => 28.0,
            BtnSize::Xs => 24.0,
        }
    }

    fn padding(self) -> f32 {
        match self {
            BtnSize::Normal => 13.0,
            BtnSize::Small => 10.0,
            BtnSize::Xs => 8.0,
        }
    }

    fn font(self) -> f32 {
        match self {
            BtnSize::Normal => 13.0,
            BtnSize::Small => 12.5,
            BtnSize::Xs => 11.5,
        }
    }

    fn radius(self) -> f32 {
        match self {
            BtnSize::Normal => 8.0,
            BtnSize::Small => 6.0,
            BtnSize::Xs => 5.0,
        }
    }
}

/// 按钮外观（不绑定事件，事件由调用方加）。
pub fn button(text: impl Into<SharedString>, kind: BtnKind, size: BtnSize, disabled: bool, p: &Palette) -> Div {
    let (bg, border, fg) = match kind {
        BtnKind::Default => (p.surface, p.line_2, p.text),
        BtnKind::Primary => (p.accent, p.accent, 0xffffff),
        BtnKind::Ghost => (p.surface, p.surface, p.text_2),
        BtnKind::Danger => (p.surface, p.line_2, p.bad),
    };
    div()
        .h(px(size.height()))
        .px(px(size.padding()))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(size.radius()))
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(border))
        .text_size(px(size.font()))
        .text_color(rgb(fg))
        .when(disabled, |d| d.opacity(0.45))
        .child(div().text_size(px(size.font())).text_color(rgb(fg)).child(text.into()))
}

/// 时间点胶囊 `.chip`：高 26，等宽 12px，右侧可带删。
pub fn chip(text: impl Into<SharedString>, removable: bool, p: &Palette) -> Div {
    let base = div()
        .h(px(26.0))
        .pl(px(9.0))
        .pr(px(5.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .rounded(px(7.0))
        .bg(rgb(p.surface_2))
        .border_1()
        .border_color(rgb(p.line_2))
        .text_size(px(12.0))
        .text_color(rgb(p.text))
        .child(div().text_size(px(12.0)).text_color(rgb(p.text)).child(text.into()));
    if removable {
        base
    } else {
        base.pr(px(9.0))
    }
}

/// 虚线胶囊「＋ 添加」。
pub fn chip_add(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .h(px(26.0))
        .px(px(9.0))
        .flex()
        .items_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(rgb(p.line_2))
        .border_dashed()
        .text_size(px(12.0))
        .text_color(rgb(p.text_3))
        .child(div().text_size(px(12.0)).text_color(rgb(p.text_3)).child(text.into()))
}

/// 面板 `.panel`：1px 描边 + 10 圆角，标题行 34px。
pub fn panel_header(title: impl Into<SharedString>, right: Option<AnyElement>, p: &Palette) -> Div {
    let mut row = div()
        .h(px(34.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(rgb(p.line))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(p.text_2))
                .child(title.into()),
        )
        .child(div().flex_1());
    if let Some(element) = right {
        row = row.child(element);
    }
    row
}

/// 输入框外框：32 高、8 圆角、1px 描边；聚焦时换强调色。
pub fn input_frame(focused: bool, readonly: bool, invalid: bool, p: &Palette) -> Div {
    let bg = if readonly { p.surface_2 } else { p.surface };
    let border = if invalid {
        p.bad
    } else if focused {
        p.accent
    } else {
        p.line_2
    };
    div()
        .h(px(32.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(8.0))
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(border))
}

/// 头像方块 `.ava`：26×26、7 圆角。
pub fn avatar(letter: impl Into<SharedString>, fg: u32, bg: u32, border: u32) -> Div {
    div()
        .w(px(26.0))
        .h(px(26.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(border))
        .text_size(px(11.5))
        .text_color(rgb(fg))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgb(fg))
                .child(letter.into()),
        )
}

/// 把两个颜色按比例混合（用于光晕、浅底等原型里用 `color-mix` 的地方）。
pub fn mix(color: u32, onto: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let blend = |shift: u32| -> u32 {
        let a = (color >> shift) & 0xff;
        let b = (onto >> shift) & 0xff;
        let value = a as f32 * (1.0 - amount) + b as f32 * amount;
        (value.round() as u32) << shift
    };
    blend(16) | blend(8) | blend(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::LIGHT;

    #[test]
    fn 按钮尺寸与原型一致() {
        assert_eq!(BtnSize::Normal.height(), 32.0);
        assert_eq!(BtnSize::Small.height(), 28.0);
        assert_eq!(BtnSize::Xs.height(), 24.0);
        assert_eq!(BtnSize::Normal.padding(), 13.0);
        assert_eq!(BtnSize::Small.font(), 12.5);
        assert_eq!(BtnSize::Xs.radius(), 5.0);
    }

    #[test]
    fn 混色在两色之间() {
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
        assert_eq!(mix(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(mix(0x000000, 0xffffff, 1.0), 0xffffff);
    }

    #[test]
    fn 浅色令牌下混色不越界() {
        let glow = mix(LIGHT.ok, LIGHT.surface, 0.86);
        // 结果必须比原色更接近白
        assert!(glow > LIGHT.ok);
        assert!(glow <= 0xffffff);
    }
}
