//! 基础控件：与 `design/v2/proto.css` 的组件一一对应。
//!
//! 每条规则后面都标了原型里的类名与尺寸，方便对照验收；改样式请先改原型。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::{App, ClickEvent, Context, ElementId, IntoElement, RenderOnce, Window};
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


/// 把 `cx.listener(...)` 的写法适配成组件库的回调签名。
///
/// 库的 `Button::on_click` 要 `Fn(&ClickEvent, &mut Window, &mut App)`，
/// 而我们全项目的写法都是 `cx.listener(|this, event, window, cx| ...)`。
/// 有了这层，调用点只改一个函数名，语义完全不变。
pub fn click<V: 'static>(
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, &ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let view = cx.entity();
    move |event, window, app| {
        view.update(app, |this, cx| handler(this, event, window, cx));
    }
}

/// 按钮：渲染完全交给组件库的 [`Button`]，这里只做一层「构建器适配」。
///
/// 为什么需要这层薄壳（30 行）：
/// - 库的 `Button` 是 `RenderOnce`，**没有** `Element` 实现，所以不能 `.id()` 之后再链
///   `.on_click()`（`Stateful<Button>` 的 trait bound 不满足）；id 必须在构造时给；
/// - 全项目 24 处调用点的写法是 `.id(x).cursor_pointer().on_click(...)`，
///   其中 `on_click` 收的是 `cx.listener(...)` 风格的回调。
/// 所以用 `Btn` 收下这些链式调用，`render` 时原样转交给 `Button`。
/// 外观（变体/尺寸/hover/焦点环/禁用态）全部来自库，我们没有自己画一像素。
#[derive(IntoElement)]
pub struct Btn {
    id: ElementId,
    text: SharedString,
    kind: BtnKind,
    size: BtnSize,
    disabled: bool,
    children: Vec<AnyElement>,
    on_click: Option<std::rc::Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl Btn {
    /// 覆盖元素 id（调用点都会显式给一个，便于点击态与测试定位）。
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    /// 库按钮自带 `cursor_pointer`，这里保留链式写法而已。
    pub fn cursor_pointer(self) -> Self {
        self
    }

    /// 追加子元素（例如带图标的「拾取」按钮）。
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// 条件构建，与 gpui 的 `.when(...)` 语义一致。
    pub fn when(mut self, condition: bool, build: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            self = build(self);
        }
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for Btn {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut button = Button::new(self.id).label(self.text);
        button = match self.kind {
            BtnKind::Default => button,
            BtnKind::Primary => button.primary(),
                BtnKind::Danger => button.danger(),
        };
        button = match self.size {
            BtnSize::Normal => button,
            BtnSize::Small => button.small(),
            BtnSize::Xs => button.xsmall(),
        };
        button = button.disabled(self.disabled);
        for child in self.children {
            button = button.child(child);
        }
        if let Some(handler) = self.on_click {
            button = button.on_click(move |event, window, cx| handler(event, window, cx));
        }
        button
    }
}

/// 造一个按钮。
///
/// 语气与尺寸映射到库的变体：Default→secondary、Primary、Ghost、Danger；
/// Normal→medium、Small→small、Xs→xsmall。
pub fn button(
    text: impl Into<SharedString>,
    kind: BtnKind,
    size: BtnSize,
    disabled: bool,
    _p: &Palette,
) -> Btn {
    let text = text.into();
    Btn {
        id: ElementId::Name(SharedString::from(format!("btn:{text}"))),
        text,
        kind,
        size,
        disabled,
        children: Vec::new(),
        on_click: None,
    }
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
