//! GPUI Kit 内存/还原度 spike
//!
//! 目的只有三个：
//! 1. 量出「窗口开着」与「最小化后」的进程内存（工作集）
//! 2. 量出 release 二进制体积
//! 3. 目测中文小字号、圆角、描边、明暗切换在 Windows 上的真实观感
//!
//! 界面刻意按 `design/v2/10-exe.html` 的主界面搭一个缩影：
//! 264 侧栏 + 主区三块面板 + 38 底部状态栏。不用真实业务逻辑。

use gpui_kit::*;
use gpui_kit::base::StyledExt;

const BG: u32 = 0xf6f7f9;
const SURFACE: u32 = 0xffffff;
const SURFACE_2: u32 = 0xfafbfc;
const SURFACE_3: u32 = 0xf1f3f6;
const LINE: u32 = 0xe6e8ec;
const LINE_2: u32 = 0xd8dbe1;
const TEXT: u32 = 0x131619;
const TEXT_2: u32 = 0x545a63;
const TEXT_3: u32 = 0x868d97;
const ACCENT: u32 = 0x2563eb;
const OK: u32 = 0x0d9488;

struct Spike {
    dark: bool,
}

impl Spike {
    fn new() -> Self {
        Self { dark: false }
    }

    fn palette(&self) -> (u32, u32) {
        if self.dark {
            (0x101317, 0x191d23)
        } else {
            (BG, SURFACE)
        }
    }
}

/// 一个账号行：头像方块 + 姓名 / 班级 + 状态点
fn account_row(name: &'static str, sub: &'static str, tone: u32, selected: bool) -> impl IntoElement {
    let base = div()
        .h_flex()
        .w_full()
        .gap_2()
        .px_2()
        .py(px(7.0))
        .rounded(px(8.0));
    let base = if selected {
        base.bg(rgb(SURFACE)).border_1().border_color(rgb(LINE))
    } else {
        base
    };
    base.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(26.0))
                .h(px(26.0))
                .rounded(px(7.0))
                .bg(rgb(0xeff4ff))
                .text_color(rgb(ACCENT))
                .text_size(px(11.5))
                .child(name.chars().next().unwrap_or('?').to_string()),
        )
        .child(
            div()
                .v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT))
                        .child(name),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(sub),
                ),
        )
        .child(div().w(px(7.0)).h(px(7.0)).rounded_full().bg(rgb(tone)))
}

/// 一块面板：标题行 + 内容
fn panel(title: &'static str, hint: &'static str, body: impl IntoElement) -> impl IntoElement {
    div()
        .v_flex()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(SURFACE))
        .overflow_hidden()
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .px_3()
                .py(px(8.0))
                .border_b_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(TEXT_2))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(hint),
                ),
        )
        .child(body)
}

fn field(label: &'static str, value: &'static str) -> impl IntoElement {
    div()
        .v_flex()
        .flex_1()
        .gap_1()
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgb(TEXT_2))
                .child(label),
        )
        .child(
            div()
                .w_full()
                .h(px(32.0))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(LINE_2))
                .bg(rgb(SURFACE))
                .text_size(px(12.0))
                .text_color(rgb(TEXT))
                .child(value),
        )
}

/// 窗口按钮：46×36，直角，悬停仅关闭按钮变红
fn wbtn(area: WindowControlArea, glyph: &'static str, danger: bool) -> impl IntoElement {
    div()
        .w(px(46.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.0))
        .text_color(rgb(TEXT_2))
        .window_control_area(area)
        .hover(move |s| {
            if danger {
                s.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))
            } else {
                s.bg(rgb(SURFACE_3))
            }
        })
        .child(glyph)
}

impl Render for Spike {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (bg, surface) = self.palette();

        let accounts = [
            ("张明远", "2023 级计算机 1 班 · 下次 21:31", OK, true),
            ("林可欣", "需重新登录 · 已暂停", 0xb45309, false),
            ("陈子谦", "已暂停 · 08:00", TEXT_3, false),
            ("黄思远", "2023 级软件 2 班 · 明天 07:30", OK, false),
            ("吴桐", "2023 级软件 2 班 · 今天 22:00", OK, false),
            ("许嘉禾", "2022 级计算机 3 班 · 明天 07:30", OK, false),
        ];

        let list = accounts.iter().enumerate().fold(
            div().v_flex().w_full().gap(px(2.0)).px_2(),
            |acc, (i, (name, sub, tone, sel))| {
                let _ = i;
                acc.child(account_row(name, sub, *tone, *sel))
            },
        );

        let log_lines = [
            "21:31:02  用户与班级：张明远 / 2000001",
            "21:31:02  检测到签到项 88213（punchs）",
            "21:31:03  网络请求成功",
            "21:31:03  本次签到成功 · 定位 39.9088, 116.3974",
            "21:31:03  下次执行 21:31（明天）",
            "--",
            "07:30:01  用户与班级：张明远 / 2000001",
            "07:30:02  检测到签到项 88213（punchs）",
            "07:30:02  本次签到成功 · 定位 39.9088, 116.3974",
        ];
        let log = log_lines.iter().fold(
            div().v_flex().w_full().gap(px(1.0)).px_3().py_2().bg(rgb(SURFACE_2)),
            |acc, line| {
                acc.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(TEXT_2))
                        .child(*line),
                )
            },
        );

        div()
            .v_flex()
            .size_full()
            .bg(rgb(bg))
            .text_size(px(13.0))
            .text_color(rgb(TEXT))
            // 标题栏 36：整条可拖拽，右侧是全高的窗口按钮
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .h(px(36.0))
                    .items_center()
                    .bg(rgb(SURFACE_2))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    // 拖拽区（图标 + 标题 + 状态 pill 都在里面）
                    .child(
                        div()
                            .h_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .pl_3()
                            .pr_2()
                            .gap_2()
                            .items_center()
                            .window_control_area(WindowControlArea::Drag)
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT_2))
                                    .child("班级魔方自动签到"),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .h(px(19.0))
                                    .flex()
                                    .items_center()
                                    .rounded_full()
                                    .bg(rgb(0xe9faf6))
                                    .text_size(px(10.0))
                                    .text_color(rgb(OK))
                                    .child("后台运行中"),
                            ),
                    )
                    // 窗口按钮 46×36，紧贴右边缘
                    .child(
                        wbtn(WindowControlArea::Min, "─", false),
                    )
                    .child(
                        wbtn(WindowControlArea::Max, "□", false),
                    )
                    .child(
                        wbtn(WindowControlArea::Close, "✕", true),
                    ),
            )
            // 主体
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .min_h_0()
                    // 侧栏 264
                    .child(
                        div()
                            .v_flex()
                            .w(px(264.0))
                            .h_full()
                            .py_2()
                            .bg(rgb(SURFACE_2))
                            .border_r_1()
                            .border_color(rgb(LINE))
                            .child(list),
                    )
                    // 主区
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(LINE))
                                    .child(
                                        div()
                                            .v_flex()
                                            .child(
                                                div()
                                                    .text_size(px(16.0))
                                                    .child("张明远"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .text_color(rgb(TEXT_3))
                                                    .child("2023 级计算机 1 班 · 今天已签到 1 次 · 下次 21:31"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .px_3()
                                            .h(px(28.0))
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .rounded(px(8.0))
                                            .bg(rgb(ACCENT))
                                            .text_color(rgb(0xffffff))
                                            .text_size(px(12.0))
                                            .child("立即签到"),
                                    ),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_h_0()
                                    .gap_3()
                                    .p_4()
                                    .child(panel(
                                        "签到地点与通知",
                                        "精度 30 米",
                                        div()
                                            .h_flex()
                                            .gap_2()
                                            .p_3()
                                            .child(field("纬度", "39.908720"))
                                            .child(field("经度", "116.397450"))
                                            .child(field("精度", "30 米")),
                                    ))
                                    .child(panel(
                                        "每天执行时间",
                                        "按北京时间 UTC+8",
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .p_3()
                                            .child(
                                                div()
                                                    .px_2()
                                                    .h(px(26.0))
                                                    .flex()
                                                    .items_center()
                                                    .rounded(px(7.0))
                                                    .border_1()
                                                    .border_color(rgb(LINE_2))
                                                    .text_size(px(12.0))
                                                    .child("07:30:00"),
                                            )
                                            .child(
                                                div()
                                                    .px_2()
                                                    .h(px(26.0))
                                                    .flex()
                                                    .items_center()
                                                    .rounded(px(7.0))
                                                    .border_1()
                                                    .border_color(rgb(LINE_2))
                                                    .text_size(px(12.0))
                                                    .child("21:31:00"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .text_color(rgb(TEXT_3))
                                                    .child("失败重试 3 次，间隔 5 分钟"),
                                            ),
                                    ))
                                    .child(panel("运行日志", "复制 · 清空 · 打开目录", log)),
                            ),
                    ),
            )
            // 状态栏 38
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .h(px(38.0))
                    .px_4()
                    .gap_3()
                    .items_center()
                    .bg(rgb(SURFACE_2))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_2))
                    .child("运行中")
                    .child("下次 21:31 · 还有 4 小时 12 分")
                    .child(
                        div()
                            .flex_1()
                            .h(px(1.0))
                            .bg(rgb(SURFACE_3)),
                    )
                    .child("无未保存的修改"),
            )
    }
}

fn main() {
    println!("spike-gpui pid={} 已启动", std::process::id());
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(80.0), px(60.0)),
            size: size(px(1160.0), px(800.0)),
        })),
        // 去掉系统标题栏，只留自绘的 36px 一条
        titlebar: Some(TitlebarOptions {
            title: Some("班级魔方自动签到".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        ..Default::default()
    };
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Spike::new()))
            .expect("打开窗口失败");
    });
}
