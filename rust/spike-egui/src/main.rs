//! egui 内存/还原度 spike（eframe 0.29）
//!
//! 与 `spike-gpui` 同一个界面缩影：36 标题栏 / 264 侧栏 / 三块面板 / 38 状态栏，全中文。
//! 只用来量内存与看观感，不含业务逻辑。

use eframe::egui;
use egui::{
    Color32, FontData, FontDefinitions, FontFamily, Margin, RichText, Rounding, Stroke, Vec2,
};

const BG: Color32 = Color32::from_rgb(0xf6, 0xf7, 0xf9);
const SURFACE: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
const SURFACE_2: Color32 = Color32::from_rgb(0xfa, 0xfb, 0xfc);
const LINE: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xec);
const LINE_2: Color32 = Color32::from_rgb(0xd8, 0xdb, 0xe1);
const TEXT: Color32 = Color32::from_rgb(0x13, 0x16, 0x19);
const TEXT_2: Color32 = Color32::from_rgb(0x54, 0x5a, 0x63);
const TEXT_3: Color32 = Color32::from_rgb(0x86, 0x8d, 0x97);
const ACCENT: Color32 = Color32::from_rgb(0x25, 0x63, 0xeb);
const OK: Color32 = Color32::from_rgb(0x0d, 0x94, 0x88);
const WARN: Color32 = Color32::from_rgb(0xb4, 0x53, 0x09);

struct Spike {
    printed: bool,
    accounts: Vec<(&'static str, &'static str, Color32, bool)>,
    log: Vec<&'static str>,
}

impl Default for Spike {
    fn default() -> Self {
        Self {
            printed: false,
            accounts: vec![
                ("张明远", "2023 级计算机 1 班 · 下次 21:31", OK, true),
                ("林可欣", "需重新登录 · 已暂停", WARN, false),
                ("陈子谦", "已暂停 · 08:00", TEXT_3, false),
                ("黄思远", "2023 级软件 2 班 · 明天 07:30", OK, false),
                ("吴桐", "2023 级软件 2 班 · 今天 22:00", OK, false),
                ("许嘉禾", "2022 级计算机 3 班 · 明天 07:30", OK, false),
            ],
            log: vec![
                "21:31:02  用户与班级：张明远 / 2000001",
                "21:31:02  检测到签到项 88213（punchs）",
                "21:31:03  网络请求成功",
                "21:31:03  本次签到成功 · 定位 39.9088, 116.3974",
                "21:31:03  下次执行 21:31（明天）",
                "--",
                "07:30:01  用户与班级：张明远 / 2000001",
                "07:30:02  检测到签到项 88213（punchs）",
                "07:30:02  本次签到成功 · 定位 39.9088, 116.3974",
            ],
        }
    }
}

fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
    ];
    let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else {
        eprintln!("未找到中文字体，跳过");
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("zh".to_owned(), FontData::from_owned(bytes));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "zh".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn apply_style(ctx: &egui::Context) {
    // spike 环境里 eframe 探测到的 scale factor 异常（内容被压到 1%），先强制 1.0
    ctx.set_pixels_per_point(1.0);
    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = SURFACE_2;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    ctx.set_style(style);
}

fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, LINE))
        .rounding(Rounding::same(10.0))
}

fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(11.5).color(TEXT_2));
        egui::Frame::none()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, LINE_2))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.set_min_width(110.0);
                ui.label(RichText::new(value).size(12.0).color(TEXT));
            });
    });
}

impl eframe::App for Spike {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.printed {
            self.printed = true;
            println!(
                "DIAG ppp={} screen_rect={:?} available={:?}",
                ctx.pixels_per_point(),
                ctx.screen_rect(),
                ctx.available_rect()
            );
        }
        egui::TopBottomPanel::top("titlebar")
            .exact_height(36.0)
            .frame(
                egui::Frame::none()
                    .fill(SURFACE_2)
                    .stroke(Stroke::new(1.0, LINE))
                    .inner_margin(Margin::symmetric(12.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("班级魔方自动签到").size(12.0).color(TEXT_2));
                    ui.add_space(6.0);
                    egui::Frame::none()
                        .fill(Color32::from_rgb(0xe9, 0xfa, 0xf6))
                        .rounding(Rounding::same(9.0))
                        .inner_margin(Margin::symmetric(7.0, 2.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("后台运行中").size(10.0).color(OK));
                        });
                });
            });

        egui::TopBottomPanel::bottom("status")
            .exact_height(38.0)
            .frame(
                egui::Frame::none()
                    .fill(SURFACE_2)
                    .stroke(Stroke::new(1.0, LINE))
                    .inner_margin(Margin::symmetric(16.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("运行中").size(11.5).color(TEXT_2));
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("下次 21:31 · 还有 4 小时 12 分")
                            .size(11.5)
                            .color(TEXT_2),
                    );
                });
            });

        egui::SidePanel::left("side")
            .exact_width(264.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(SURFACE_2)
                    .stroke(Stroke::new(1.0, LINE))
                    .inner_margin(Margin::symmetric(8.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("账号 2 / 3").size(11.0).color(TEXT_3));
                ui.add_space(4.0);
                for (name, sub, tone, selected) in &self.accounts {
                    egui::Frame::none()
                        .fill(if *selected { SURFACE } else { SURFACE_2 })
                        .stroke(Stroke::new(if *selected { 1.0 } else { 0.0 }, LINE))
                        .rounding(Rounding::same(8.0))
                        .inner_margin(Margin::symmetric(8.0, 7.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(7.0),
                                    Color32::from_rgb(0xef, 0xf4, 0xff),
                                );
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    name.chars().next().unwrap_or('?').to_string(),
                                    egui::FontId::proportional(11.5),
                                    ACCENT,
                                );
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(*name).size(12.5).color(TEXT));
                                    ui.label(RichText::new(*sub).size(10.5).color(TEXT_3));
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            Vec2::splat(7.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().circle_filled(rect.center(), 3.5, *tone);
                                    },
                                );
                            });
                        });
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG))
            .show(ctx, |ui| {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("张明远").size(16.0).color(TEXT));
                        ui.label(
                            RichText::new("2023 级计算机 1 班 · 今天已签到 1 次 · 下次 21:31")
                                .size(11.5)
                                .color(TEXT_3),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new("立即签到").size(12.0).color(Color32::WHITE),
                        )
                        .fill(ACCENT)
                        .rounding(Rounding::same(8.0));
                        let _ = ui.add_sized([72.0, 28.0], btn);
                    });
                });
                ui.add_space(12.0);

                let w = ui.available_width();
                card().show(ui, |ui| {
                    ui.set_width(w);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("签到地点与通知").size(12.0).color(TEXT_2));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("精度 30 米").size(10.5).color(TEXT_3));
                        });
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        field(ui, "纬度", "39.908720");
                        field(ui, "经度", "116.397450");
                        field(ui, "精度", "30 米");
                    });
                });
                ui.add_space(12.0);

                let w = ui.available_width();
                card().show(ui, |ui| {
                    ui.set_width(w);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("每天执行时间").size(12.0).color(TEXT_2));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("按北京时间 UTC+8").size(10.5).color(TEXT_3));
                        });
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        for t in ["07:30:00", "21:31:00"] {
                            egui::Frame::none()
                                .fill(SURFACE)
                                .stroke(Stroke::new(1.0, LINE_2))
                                .rounding(Rounding::same(7.0))
                                .inner_margin(Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(t).size(12.0).color(TEXT));
                                });
                        }
                        ui.label(
                            RichText::new("失败重试 3 次，间隔 5 分钟")
                                .size(11.5)
                                .color(TEXT_3),
                        );
                    });
                });
                ui.add_space(12.0);

                let w = ui.available_width();
                card().show(ui, |ui| {
                    ui.set_width(w);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("运行日志").size(12.0).color(TEXT_2));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new("复制 · 清空 · 打开目录").size(10.5).color(TEXT_3),
                            );
                        });
                    });
                    ui.separator();
                    let lw = ui.available_width();
                    egui::Frame::none()
                        .fill(SURFACE_2)
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(lw);
                            for line in &self.log {
                                ui.label(RichText::new(*line).monospace().size(11.0).color(TEXT_2));
                            }
                        });
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    println!("spike-egui pid={} 已启动", std::process::id());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1160.0, 800.0])
            .with_title("班级魔方自动签到 · egui spike"),
        ..Default::default()
    };
    eframe::run_native(
        "spike-egui",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            apply_style(&cc.egui_ctx);
            Ok(Box::new(Spike::default()))
        }),
    )
}
