//! 最小对照用例：只画一行中文 + 一个按钮，用来判断空白窗口是布局问题还是渲染问题。

use eframe::egui;

struct Simple;

impl eframe::App for Simple {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("最小用例：你好，班级魔方");
            ui.label("如果这行字正常显示，说明渲染没问题，是复杂布局写错了。");
            let _ = ui.button("一个按钮");
        });
    }
}

fn main() -> eframe::Result<()> {
    println!("simple pid={}", std::process::id());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 400.0])
            .with_title("simple-egui"),
        ..Default::default()
    };
    eframe::run_native(
        "simple",
        options,
        Box::new(|cc| {
            if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
                let mut fonts = egui::FontDefinitions::default();
                fonts
                    .font_data
                    .insert("zh".to_owned(), egui::FontData::from_owned(bytes));
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .insert(0, "zh".to_owned());
                }
                cc.egui_ctx.set_fonts(fonts);
            }
            Ok(Box::new(Simple))
        }),
    )
}
