//! 设置窗口（屏 04 / 05）：860×560，左侧四项导航。
//!
//! 画成主窗口之上的覆盖层（`Overlay::Settings`），不另开系统窗口：
//! 双窗口要额外处理焦点与 z-order，收益只有「设置能拖动」这一点。

use gpui_kit::base::StyledExt;
use gpui_kit::prelude::*;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{div, px, rgb, AnyElement, Context, Div};

use crate::theme::*;
use crate::ui::main_view::{AppView, Overlay, SettingsTab};
use crate::ui::widgets::*;

pub fn render(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let body = match view.settings_tab {
        SettingsTab::General => general(view, p, cx),
        SettingsTab::Data => data(view, p, cx),
        SettingsTab::About => about(p),
    };

    // 图标改用 gpui-kit 内置的 Lucide 图标集（与 ElegantClipboard 同一套），
    // 不再自己维护 assets/icons/*.svg
    let items = [
        (SettingsTab::General, IconName::Settings, "常规"),
        (SettingsTab::Data, IconName::HardDrive, "数据"),
        (SettingsTab::About, IconName::Info, "关于"),
    ];
    let mut nav = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(10.0))
        .w(px(176.0))
        .h_full()
        .bg(rgb(p.surface_2))
        .border_r_1()
        .border_color(rgb(p.line));
    for (tab, icon, title) in items {
        let active = view.settings_tab == tab;
        let mut item = div()
            .id(gpui_kit::SharedString::from(format!("set-tab-{title}")))
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(30.0))
            .px(px(9.0))
            .rounded(px(7.0))
            .cursor_pointer()
            // 选中项用强调色实底 + 白字（参考 ElegantClipboard 的导航）
            .bg(rgb(if active { p.accent } else { p.surface_2 }))
            .border_1()
            .border_color(rgb(if active { p.accent } else { p.surface_2 }))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.settings_tab = tab;
                cx.notify();
            }))
            .child(
                Icon::new(icon)
                    .w(px(14.0))
                    .h(px(14.0))
                    .text_color(rgb(if active { 0xffffff } else { p.text_3 })),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgb(if active { 0xffffff } else { p.text_2 }))
                    .child(title),
            );

        if !active {
            item = item.hover(|style| style.bg(rgb(p.surface_3)));
        }
        nav = nav.child(item);
    }

    // 底部版本信息（参考图的左下角）
    nav = nav
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .pt(px(10.0))
                .border_t_1()
                .border_color(rgb(p.line))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(p.text_3))
                        .child(format!("版本号  v{}", env!("CARGO_PKG_VERSION"))),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(p.text_3))
                        .child("Rust 版 · MIT License"),
                ),
        );

    // 独立窗口：铺满整窗，不再画遮罩与自绘标题栏
    // （自绘标题栏挡不住拖动，圆角在系统窗口里也多余）
    div()
        .size_full()
        .flex()
        .bg(rgb(p.surface))
        .child(nav)
        .child(body)
        .into_any_element()
}

fn pane(p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .h_full()
        .p(px(16.0))
        .gap(px(2.0))
}

fn group(p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(p.line))
        .px(px(14.0))
}

fn heading(text: &'static str, p: &Palette) -> Div {
    div().text_size(px(13.0)).text_color(rgb(p.text)).child(text)
}

fn sub(text: &'static str, p: &Palette) -> Div {
    div()
        .text_size(px(11.5))
        .text_color(rgb(p.text_3))
        .mb(px(8.0))
        .child(text)
}

fn row(title: &'static str, description: &'static str, control: AnyElement, p: &Palette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .py(px(11.0))
        .border_b_1()
        .border_color(rgb(p.line))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(div().text_size(px(12.5)).text_color(rgb(p.text)).child(title))
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_3))
                        .child(description),
                ),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

fn general(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let autostart = view.state.settings.autostart;
    let minimize = view.state.settings.minimize_to_tray;
    let theme_index = match view.state.theme {
        ThemeMode::Light => 0,
        ThemeMode::Dark => 1,
        ThemeMode::System => 2,
    };

    let autostart_switch = switch(autostart, false, p)
        .id("set-autostart")
        .cursor_pointer()
        .on_click(cx.listener(|view, _, _, cx| {
            let next = !view.state.settings.autostart;
            view.state.settings.autostart = next;
            crate::system::autostart::set_enabled(next);
            let _ = view.state.settings.save(view.state.paths.settings_file());
            cx.notify();
        }))
        .into_any_element();

    // 关窗行为：两段式
    let mut close_seg = div().flex().gap(px(2.0)).p(px(2.0)).rounded(px(7.0)).bg(rgb(p.surface_3));
    for (index, text) in ["最小化到托盘", "直接退出"].iter().enumerate() {
        let active = (index == 0) == minimize;
        close_seg = close_seg.child(
            div()
                .id(("close-mode", index))
                .px(px(10.0))
                .py(px(3.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .bg(rgb(if active { p.surface } else { p.surface_3 }))
                .text_size(px(12.0))
                .text_color(rgb(if active { p.text } else { p.text_2 }))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.state.settings.minimize_to_tray = index == 0;
                    let _ = view.state.settings.save(view.state.paths.settings_file());
                    cx.notify();
                }))
                .child(*text),
        );
    }

    let mut theme_seg = div().flex().gap(px(2.0)).p(px(2.0)).rounded(px(7.0)).bg(rgb(p.surface_3));
    for (index, text) in ["浅色", "深色", "跟随系统"].iter().enumerate() {
        let active = index == theme_index;
        theme_seg = theme_seg.child(
            div()
                .id(("theme-mode", index))
                .px(px(10.0))
                .py(px(3.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .bg(rgb(if active { p.surface } else { p.surface_3 }))
                .text_size(px(12.0))
                .text_color(rgb(if active { p.text } else { p.text_2 }))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.state.theme = ThemeMode::all()[index];
                    view.dark = view.state.theme.is_dark(system_is_dark());
                    cx.notify();
                }))
                .child(*text),
        );
    }

    pane(p)
        .child(heading("启动与窗口", p))
        .child(sub("这些开关只影响本机，不随账号同步。", p))
        .child(
            group(p)
                .child(row(
                    "开机自动启动",
                    "登录 Windows 后静默启动，不弹窗，只在托盘运行",
                    autostart_switch,
                    p,
                ))
                .child(row(
                    "关闭窗口时",
                    "默认最小化到托盘，定时任务继续运行；要退出请用托盘菜单",
                    close_seg.into_any_element(),
                    p,
                ))
                .child(row(
                    "同时只允许一个实例",
                    "这是固定行为，不提供开关：重复启动会唤起已运行的窗口，避免重复签到",
                    // 不可关闭的项不放开关，免得看起来像「能点但点不动」
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_3))
                        .child("始终生效")
                        .into_any_element(),
                    p,
                )),
        )
        .child(div().mt(px(16.0)).child(heading("外观", p)))
        .child(sub("与 Web 端使用同一套令牌，切换后立即生效。", p))
        .child(group(p).child(row(
            "主题",
            "「跟随系统」会读取 Windows 的浅色 / 深色设置",
            theme_seg.into_any_element(),
            p,
        )))
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pt(px(14.0))
                .mt(px(14.0))
                .border_t_1()
                .border_color(rgb(p.line))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_3))
                        .child("设置即时保存，无需确认"),
                )
                .child(
                    button("完成", BtnKind::Primary, BtnSize::Small, false, p)
                        .id("settings-done")
                        .cursor_pointer()
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.close_settings(window, cx);
                        })),
                ),
        )
        .into_any_element()
}

fn data(view: &mut AppView, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let dir = view.state.data_dir().display().to_string();
    let logs = view.state.logs.len();

    pane(p)
        .child(heading("数据目录", p))
        .child(sub("便携模式下自动使用 exe 同级的 data 目录，不写注册表。", p))
        .child(
            group(p)
                .child(row(
                    "当前路径",
                    "配置、Cookie、日志都在这里",
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(p.text_2))
                                .child(dir.clone()),
                        )
                        .child(
                            button("打开", BtnKind::Default, BtnSize::Small, false, p)
                                .id("open-data-dir")
                                .cursor_pointer()
                                .on_click(cx.listener(|view, _, _, cx| {
                                    let path = view.state.data_dir().display().to_string();
                                    let _ = crate::ui::main_view::open_url(&path);
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                    p,
                ))
                .child(row(
                    "本次日志",
                    "运行日志随进程重启清空，历史写入 logs/ 目录",
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_2))
                        .child(format!("{logs} 行"))
                        .into_any_element(),
                    p,
                )),
        )
        .child(div().mt(px(16.0)).child(heading("备份与迁移", p)))
        .child(sub("导出文件包含账号与配置，不含日志。请勿分享给他人。", p))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    button("导出配置（.json）", BtnKind::Default, BtnSize::Small, false, p)
                        .id("export-config")
                        .cursor_pointer()
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.export_config(cx);
                            cx.notify();
                        })),
                )
                .child(
                    button("导入配置", BtnKind::Default, BtnSize::Small, false, p)
                        .id("import-config-settings")
                        .cursor_pointer()
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.import_config(cx);
                            cx.notify();
                        })),
                ),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .pt(px(14.0))
                .mt(px(14.0))
                .border_t_1()
                .border_color(rgb(p.line))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_3))
                        .child("单实例 · 本地存储 · 不收集任何数据"),
                )
                .child(
                    button("完成", BtnKind::Primary, BtnSize::Small, false, p)
                        .id("settings-done-data")
                        .cursor_pointer()
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.close_settings(window, cx);
                        })),
                ),
        )
        .into_any_element()
}

fn about(p: &Palette) -> AnyElement {
    pane(p)
        .child(heading("关于", p))
        .child(
            group(p)
                .child(row(
                    "班级魔方自动签到",
                    "Rust 版 · 每天在固定时间点自动签到",
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_2))
                        .child(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .into_any_element(),
                    p,
                ))
                .child(row(
                    "开源许可",
                    "MIT License",
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_2))
                        .child("MIT")
                        .into_any_element(),
                    p,
                )),
        )
        .child(
            div()
                .mt(px(12.0))
                .text_size(px(11.5))
                .text_color(rgb(p.text_3))
                .child("本工具只做本地签到提交，不上传任何数据；账号凭证仅保存在本机 data.json。"),
        )
        .child(div().flex_1())
        .into_any_element()
}
