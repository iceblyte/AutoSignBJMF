//! 主窗口：屏 01 / 02 / 06。
//!
//! 结构严格照 `design/v2/10-exe.html`：
//!
//! ```text
//! .win
//! ├── .wt            36px 自绘标题栏
//! └── .wb
//!     ├── .wside     264px 侧栏（账号列表 + 底部开关）
//!     └── .wmain
//!         ├── .wmh   标题行（姓名 / 状态 pill / 操作）
//!         ├── .wbody 面板区
//!         └── .sbar  38px 状态栏
//! ```

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::base::StyledExt;

use crate::state::{AccountStatus, AppState, LogKind, PillTone};
use crate::theme::*;
use crate::ui::widgets::*;

/// 可编辑字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Lat,
    Lng,
    Wx,
    Qq,
    RetryAttempts,
    RetryDelay,
    AddTime,
}

impl Field {
    fn index(self) -> usize {
        match self {
            Field::Lat => 0,
            Field::Lng => 1,
            Field::Wx => 2,
            Field::Qq => 3,
            Field::RetryAttempts => 4,
            Field::RetryDelay => 5,
            Field::AddTime => 6,
        }
    }

    pub const ALL: [Field; 7] = [
        Field::Lat,
        Field::Lng,
        Field::Wx,
        Field::Qq,
        Field::RetryAttempts,
        Field::RetryDelay,
        Field::AddTime,
    ];
}

/// 打开的面板 / 弹窗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Qr,
    Settings(SettingsTab),
    CloseHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Notify,
    Data,
    About,
}

/// 主视图。
pub struct AppView {
    pub state: AppState,
    /// 调度线程 + 托盘
    pub runtime: std::sync::Arc<crate::runtime::Runtime>,
    pub dark: bool,
    pub overlay: Overlay,
    pub active: Option<Field>,
    /// 「＋ 添加」正在输入的时间点
    pub add_time: String,
    /// 保存失败时的提示
    pub error: Option<String>,
    /// 设置窗口当前页
    pub settings_tab: SettingsTab,
    /// 扫码会话（后台线程句柄）
    pub qr: Option<crate::ui::qr_modal::QrHandle>,
    pub qr_image: Option<String>,
    pub qr_error: Option<String>,
    focus: Vec<FocusHandle>,
    /// 关窗提示勾了「不再提示」
    pub close_hint_muted: bool,
    /// 用户点了关闭按钮（由窗口回调置位，界面下一帧消费）
    pub close_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AppView {
    pub fn new(
        state: AppState,
        runtime: std::sync::Arc<crate::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        let dark = ThemeMode::System.is_dark(system_is_dark());
        let focus = (0..Field::ALL.len())
            .map(|_| cx.focus_handle())
            .collect::<Vec<_>>();
        let mut overlay = Overlay::None;
        // 截图/QA：AUTOSIGN_OPEN=settings|qr 直接打开对应覆盖层
        if let Ok(value) = std::env::var("AUTOSIGN_OPEN") {
            overlay = match value.as_str() {
                "settings" => Overlay::Settings(SettingsTab::General),
                "data" => Overlay::Settings(SettingsTab::Data),
                "qr" => Overlay::Qr,
                _ => Overlay::None,
            };
        }

        Self {
            state,
            runtime,
            dark,
            overlay,
            active: None,
            add_time: String::new(),
            error: None,
            settings_tab: SettingsTab::General,
            qr: None,
            qr_image: None,
            qr_error: None,
            focus,
            close_hint_muted: false,
            close_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 每秒把调度线程的状态与日志搬进界面。
    pub fn start_ticker(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1000))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        view.tick();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// 同步一次后台状态。
    pub fn tick(&mut self) {
        let (running, paused, next, last_exit) = self.runtime.snapshot();
        self.state.running = running;
        self.state.paused = paused;
        self.state.next_run_at = next;
        self.state.last_exit = last_exit;
        for (kind, line) in self.runtime.drain() {
            self.state.log(kind, line);
        }
        if matches!(self.overlay, Overlay::Qr) {
            crate::ui::qr_modal::poll(self);
        }
        // 关窗：默认隐藏到托盘（调度继续跑），选了「直接退出」才真退出
        if self
            .close_signal
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            if self.state.settings.minimize_to_tray {
                crate::system::win::hide_main_window();
                self.state.log(LogKind::Info, "窗口已隐藏到托盘，签到继续在后台运行");
                if !self.close_hint_muted {
                    self.overlay = Overlay::CloseHint;
                }
            } else {
                std::process::exit(0);
            }
        }

        // 托盘「打开主窗口」
        if self.runtime.take_show_request() {
            crate::system::win::show_main_window();
            self.overlay = Overlay::None;
        }
    }

    fn palette(&self) -> &'static Palette {
        if self.dark {
            &DARK
        } else {
            &LIGHT
        }
    }

    fn value_of(&self, field: Field) -> String {
        let draft = &self.state.draft;
        match field {
            Field::Lat => draft.lat.clone(),
            Field::Lng => draft.lng.clone(),
            Field::Wx => draft.wx_key.clone(),
            Field::Qq => draft.qq_key.clone(),
            Field::RetryAttempts => draft.retry_attempts.clone(),
            Field::RetryDelay => draft.retry_delay.clone(),
            Field::AddTime => self.add_time.clone(),
        }
    }

    fn set_value(&mut self, field: Field, value: String) {
        if field == Field::AddTime {
            self.add_time = value;
            return;
        }
        self.state.edit(|draft| match field {
            Field::Lat => draft.lat = value,
            Field::Lng => draft.lng = value,
            Field::Wx => draft.wx_key = value,
            Field::Qq => draft.qq_key = value,
            Field::RetryAttempts => draft.retry_attempts = value,
            Field::RetryDelay => draft.retry_delay = value,
            Field::AddTime => {}
        });
    }

    fn on_key(&mut self, field: Field, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let mut value = self.value_of(field);
        match key {
            "backspace" => {
                value.pop();
            }
            "escape" => {
                self.active = None;
                cx.notify();
                return;
            }
            "enter" => {
                if field == Field::AddTime {
                    let raw = self.add_time.clone();
                    if !raw.trim().is_empty() {
                        match self.state.add_time(&raw) {
                            Ok(()) => {
                                self.add_time.clear();
                                self.error = None;
                            }
                            Err(error) => self.error = Some(error.0),
                        }
                    }
                }
                cx.notify();
                return;
            }
            "space" => value.push(' '),
            "tab" => {}
            _ => {
                if let Some(text) = event.keystroke.key_char.as_ref() {
                    // 只接受可打印字符；中文输入法在本工具里没有场景（全是坐标/数字/Key）
                    if !text.is_empty() && !text.chars().any(|c| c.is_control()) {
                        value.push_str(text);
                    }
                }
            }
        }
        self.set_value(field, value);
        cx.notify();
    }

    /// 扫码添加：真的去拉一张二维码（后台线程），成功后插入账号。
    fn start_qr_login(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::Qr;
        self.qr_error = None;
        self.qr_image = None;
        self.state.log(LogKind::Info, "正在获取登录二维码…");
        self.qr = Some(crate::ui::qr_modal::start(self.state.paths.config_file()));
        cx.notify();
    }

    /// 导出配置：账号 + 设置，不含日志。
    pub fn export_config(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出配置")
            .set_file_name("AutoSignBJMF-配置.json")
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        let text = crate::ui::transfer::export_bundle(&self.state.config, &self.state.settings);
        match std::fs::write(&path, text) {
            Ok(()) => self.state.log(
                LogKind::Ok,
                format!("已导出配置到 {}", path.display()),
            ),
            Err(error) => self
                .state
                .log(LogKind::Bad, format!("导出失败：{error}")),
        }
    }

    /// 导入配置：支持本程序导出格式，也支持直接选 Python 版的 data.json。
    pub fn import_config(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("导入配置")
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.state
                    .log(LogKind::Bad, format!("读取文件失败：{error}"));
                return;
            }
        };
        let (config, settings) = match crate::ui::transfer::import_bundle(&text) {
            Ok(pair) => pair,
            Err(error) => {
                self.error = Some(error.clone());
                self.state.log(LogKind::Bad, format!("导入失败：{error}"));
                return;
            }
        };
        let summary = crate::ui::transfer::describe(&config);
        if let Err(error) = config.save(self.state.paths.config_file()) {
            self.state
                .log(LogKind::Bad, format!("写入配置失败：{error}"));
            return;
        }
        if let Err(error) = settings.save(self.state.paths.settings_file()) {
            self.state
                .log(LogKind::Bad, format!("写入设置失败：{error}"));
            return;
        }
        self.state.config = config;
        self.state.settings = settings;
        self.state.selected = 0;
        self.state.reload_draft();
        self.state.log(LogKind::Ok, format!("已导入配置：{summary}"));
    }

    /// 删除当前账号（连同 Cookie 一起从 data.json 里去掉）。
    fn delete_selected(&mut self) {
        let index = self.state.selected;
        if index >= self.state.students().len() {
            return;
        }
        let removed = self.state.config.students.remove(index);
        let _ = self.state.config.save(self.state.paths.config_file());
        if self.state.students().is_empty() {
            self.state.selected = 0;
            self.state.reload_draft();
        } else {
            let next = index.min(self.state.students().len() - 1);
            self.state.select(next);
        }
        self.state.log(
            LogKind::Warn,
            format!("已删除账号 {}（Cookie 一并移除）", removed.name),
        );
    }

    fn save(&mut self, cx: &mut Context<Self>) {        match self.state.save_draft() {
            Ok(()) => {
                self.error = None;
                self.state.log(LogKind::Ok, "已保存地点、通知与重试设置");
            }
            Err(error) => {
                self.error = Some(error.0);
            }
        }
        cx.notify();
    }

    // ---------------- 标题栏 ----------------

    fn render_titlebar(&self, p: &Palette) -> AnyElement {
        let (tone, text) = self.state.title_status();
        let (fg, bg, border) = match tone {
            PillTone::Ok => (p.ok, p.ok_soft, p.ok_line),
            PillTone::Warn => (p.warn, p.warn_soft, p.warn_line),
            PillTone::Plain => (p.text_2, p.surface_3, p.surface_3),
        };

        div()
            .h(px(TITLEBAR_H))
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .bg(rgb(p.surface_2))
            .border_b_1()
            .border_color(rgb(p.line))
            // 拖拽区：logo + 标题 + 状态 pill
            .child(
                div()
                    .flex()
                    .items_center()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .pl(px(12.0))
                    .pr(px(8.0))
                    .gap(px(8.0))
                    .window_control_area(WindowControlArea::Drag)
                    .child(img("logo.png").w(px(16.0)).h(px(16.0)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(p.text_2))
                            .child("班级魔方自动签到"),
                    )
                    .child(pill(text, fg, bg, border, true)),
            )
            .child(window_button(WindowControlArea::Min, p))
            .child(window_button(WindowControlArea::Max, p))
            .child(window_button(WindowControlArea::Close, p))
            .into_any_element()
    }

    // ---------------- 侧栏 ----------------

    fn render_sidebar(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let count = self.state.students().len();
        let running = self
            .state
            .students()
            .iter()
            .enumerate()
            .filter(|(index, _)| self.state.status_of(*index) != AccountStatus::NeedsLogin)
            .count();

        let mut rows = div().flex().flex_col().w_full().gap(px(2.0)).px(px(8.0));
        if count == 0 {
            rows = rows.child(
                div()
                    .flex_1()
                    .px(px(6.0))
                    .py(px(20.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(rgb(p.line_2))
                    .border_dashed()
                    .bg(rgb(p.surface_2))
                    .child(
                        div()
                            .w(px(34.0))
                            .h(px(34.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.0))
                            .bg(rgb(p.surface))
                            .border_1()
                            .border_color(rgb(p.line))
                            .child(
                                svg()
                                    .path("icons/person-add.svg")
                                    .w(px(15.0))
                                    .h(px(15.0))
                                    .text_color(rgb(p.text_3)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(p.text_2))
                            .child("还没有账号"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .text_center()
                            .child("扫码添加第一个账号后，这里会显示状态与下次执行时间"),
                    ),
            );
        } else {
            for index in 0..count {
                rows = rows.child(self.render_account_row(index, p, cx));
            }
        }

        div()
            .w(px(SIDEBAR_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(p.surface_2))
            .border_r_1()
            .border_color(rgb(p.line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .pt(px(11.0))
                    .pb(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(p.text_3))
                            .child("账号"),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(p.text_3))
                            .child(format!("{running} / {count}")),
                    )
                    .child(div().flex_1())
                    .child(
                        button("＋ 扫码添加", BtnKind::Default, BtnSize::Xs, false, p)
                            .id("add-account")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_qr_login(window, cx);
                            })),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(rows))
            .child(self.render_sidebar_footer(p, cx))
            .into_any_element()
    }

    fn render_account_row(&self, index: usize, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let student = &self.state.students()[index];
        let name = if student.name.trim().is_empty() {
            format!("账号{}", index + 1)
        } else {
            student.name.clone()
        };
        let letter: String = name.chars().take(1).collect();
        let status = self.state.status_of(index);
        let selected = index == self.state.selected;

        let (ava_fg, ava_bg, ava_border) = match status {
            AccountStatus::NeedsLogin => (p.warn, p.warn_soft, p.warn_line),
            AccountStatus::Idle | AccountStatus::Paused => (p.text_2, p.surface_3, p.line),
            _ => (p.accent, p.accent_soft, p.accent_line),
        };
        let dot_color = match status {
            AccountStatus::Running => p.ok,
            AccountStatus::NeedsLogin | AccountStatus::Failed => p.warn,
            _ => p.text_3,
        };

        div()
            .id(("account", index))
            .flex()
            .items_center()
            .gap(px(9.0))
            .h(px(ACCOUNT_ROW_H))
            .px(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(if selected { p.line } else { p.surface_2 }))
            .bg(rgb(if selected { p.surface } else { p.surface_2 }))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(p.surface_3)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.select(index);
                this.error = None;
                cx.notify();
            }))
            .child(avatar(letter, ava_fg, ava_bg, ava_border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(p.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(p.text_3))
                            .overflow_hidden()
                            .child(self.state.subtitle_of(index)),
                    ),
            )
            .child(dot(dot_color, status == AccountStatus::Running, p))
            .into_any_element()
    }

    fn render_sidebar_footer(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let autostart = self.state.settings.autostart;
        let minimize = self.state.settings.minimize_to_tray;

        div()
            .border_t_1()
            .border_color(rgb(p.line))
            .px(px(12.0))
            .py(px(9.0))
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_2))
                    .child("开机自启")
                    .child(
                        div()
                            .id("toggle-autostart")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.settings.autostart = !this.state.settings.autostart;
                                crate::system::autostart::set_enabled(
                                    this.state.settings.autostart,
                                );
                                cx.notify();
                            }))
                            .child(switch(autostart, false, p)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_2))
                    .child("关窗最小化到托盘")
                    .child(
                        div()
                            .id("toggle-minimize")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.settings.minimize_to_tray =
                                    !this.state.settings.minimize_to_tray;
                                cx.notify();
                            }))
                            .child(switch(minimize, false, p)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pt(px(2.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child(match self.state.next_run_at {
                                Some(next) => format!("下次同步 {}", next.format("%H:%M")),
                                None => "尚未开始调度".to_string(),
                            }),
                    )
                    .child(
                        div()
                            .id("open-settings")
                            .cursor_pointer()
                            .text_size(px(11.5))
                            .text_color(rgb(p.accent))
                            .child("设置")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_tab = SettingsTab::General;
                                this.overlay = Overlay::Settings(SettingsTab::General);
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    // ---------------- 主区 ----------------

    fn render_main(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.state.has_accounts() {
            return self.render_empty(p, cx);
        }
        let index = self.state.selected;
        let student = &self.state.students()[index];
        let status = self.state.status_of(index);
        let name = if student.name.trim().is_empty() {
            format!("账号{}", index + 1)
        } else {
            student.name.clone()
        };

        let (pill_fg, pill_bg, pill_border) = match status {
            AccountStatus::Running => (p.ok, p.ok_soft, p.ok_line),
            AccountStatus::NeedsLogin | AccountStatus::Failed => (p.warn, p.warn_soft, p.warn_line),
            _ => (p.text_2, p.surface_3, p.surface_3),
        };

        let readonly = status == AccountStatus::NeedsLogin;

        let mut header = div()
            .px(px(18.0))
            .pt(px(14.0))
            .pb(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(rgb(p.line))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(rgb(p.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .mt(px(1.0))
                            .child(self.state.headline_of(index)),
                    ),
            )
            .child(pill(
                status.label(),
                pill_fg,
                pill_bg,
                pill_border,
                true,
            ));

        header = if readonly {
            header
                .child(
                    button("查看日志", BtnKind::Default, BtnSize::Small, false, p)
                        .id("view-log")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let dir = this.state.data_dir().display().to_string();
                            let _ = open_url(&dir);
                            cx.notify();
                        })),
                )
                .child(
                    button("重新扫码", BtnKind::Primary, BtnSize::Small, false, p)
                        .id("relogin")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_qr_login(window, cx);
                        })),
                )
        } else {
            header
                .child(
                    button("暂停", BtnKind::Default, BtnSize::Small, false, p)
                        .id("toggle-pause")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let next = !this.state.paused;
                            this.runtime.set_paused(next);
                            this.state.paused = next;
                            this.state.log(
                                LogKind::Info,
                                if next { "已暂停全部账号" } else { "已恢复调度" },
                            );
                            cx.notify();
                        })),
                )
                .child(
                    button("立即签到", BtnKind::Primary, BtnSize::Small, false, p)
                        .id("run-now")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.runtime.request_run_now();
                            cx.notify();
                        })),
                )
        };

        let body = div()
            .flex_1()
            .min_h_0()
            .px(px(18.0))
            .py(px(14.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .overflow_hidden()
            .when(readonly, |d| d.child(self.render_warn_alert(p)))
            .child(self.render_location_panel(p, cx, readonly))
            .child(self.render_time_panel(p, cx, readonly))
            .child(self.render_log_panel(p, cx));

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .child(body)
            .child(self.render_statusbar(p, cx, readonly))
            .into_any_element()
    }

    fn render_warn_alert(&self, p: &Palette) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(9.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(10.0))
            .bg(rgb(p.warn_soft))
            .border_1()
            .border_color(rgb(p.warn_line))
            .child(
                svg()
                    .path("icons/warn.svg")
                    .w(px(13.0))
                    .h(px(13.0))
                    .mt(px(1.0))
                    .text_color(rgb(p.warn)),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.0))
                    .text_color(rgb(p.warn))
                    .child(
                        "微信登录凭证已过期，该账号的定时签到已暂停。重新扫码后会沿用现有配置，不需要重新填地点和时间。",
                    ),
            )
            .into_any_element()
    }

    fn render_location_panel(
        &self,
        p: &Palette,
        cx: &mut Context<Self>,
        readonly: bool,
    ) -> AnyElement {
        let acc_hint = format!(
            "精度 {} 米",
            if self.state.draft.acc.trim().is_empty() {
                "30"
            } else {
                self.state.draft.acc.as_str()
            }
        );
        let mut hint = div()
            .text_size(px(10.5))
            .text_color(rgb(p.text_3))
            .child(if readonly {
                "凭证失效，暂不可编辑".to_string()
            } else {
                acc_hint.clone()
            });

        if !readonly {
            hint = hint;
        }

        let lat = self.text_field(Field::Lat, true, readonly, p, cx);
        let lng = self.text_field(Field::Lng, true, readonly, p, cx);
        let wx = self.text_field(Field::Wx, true, readonly, p, cx);
        let qq = self.text_field(Field::Qq, true, readonly, p, cx);

        let pin = svg()
            .path("icons/pin.svg")
            .w(px(13.0))
            .h(px(13.0))
            .text_color(rgb(p.text_2));

        div()
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(p.line))
            .bg(rgb(p.surface))
            .overflow_hidden()
            .child(panel_header("签到地点与通知", Some(hint.into_any_element()), p))
            .child(
                div()
                    .p(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_end()
                            .gap(px(8.0))
                            .child(field_block("纬度", lat, p))
                            .child(field_block("经度", lng, p))
                            .child(
                                button("拾取", BtnKind::Default, BtnSize::Small, readonly, p)
                                    .id("pick-location")
                                    .cursor_pointer()
                                    .child(pin)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        // 拾取：打开高德坐标拾取页（与 Python 版一致）
                                        let _ = open_url("https://lbs.amap.com/tools/picker");
                                        this.state.log(LogKind::Info, "已打开坐标拾取页");
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_end()
                            .gap(px(8.0))
                            .child(field_block_with_switch(
                                "微信通知 · Server 酱",
                                !self.state.draft.wx_key.trim().is_empty(),
                                wx,
                                p,
                                readonly,
                            ))
                            .child(field_block_with_switch(
                                "QQ 通知 · Qmsg 酱",
                                !self.state.draft.qq_key.trim().is_empty(),
                                qq,
                                p,
                                readonly,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_time_panel(&self, p: &Palette, cx: &mut Context<Self>, readonly: bool) -> AnyElement {
        let mut chips = div().flex().w_full().items_center().gap(px(7.0)).flex_wrap();
        for time in &self.state.settings.times {
            let value = time.clone();
            chips = chips.child(
                chip(time.clone(), true, p)
                    .id(SharedString::from(format!("chip-{time}")))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        match this.state.remove_time(&value) {
                            Ok(()) => this.error = None,
                            Err(error) => this.error = Some(error.0),
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(15.0))
                            .h(px(15.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .text_color(rgb(p.text_3))
                            .child("×"),
                    ),
            );
        }
        chips = chips.child(if readonly {
            chip_add("＋ 添加", p).opacity(0.45).into_any_element()
        } else {
            chip_add("＋ 添加", p)
                .id("add-time")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.active = Some(Field::AddTime);
                    window.focus(&this.focus[Field::AddTime.index()], cx);
                    cx.notify();
                }))
                .into_any_element()
        });

        let attempts = self.text_field(Field::RetryAttempts, true, readonly, p, cx);
        let delay = self.text_field(Field::RetryDelay, true, readonly, p, cx);

        let mut panel = div()
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(p.line))
            .bg(rgb(p.surface))
            .overflow_hidden()
            .child(panel_header(
                "每天执行时间",
                Some(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(p.text_3))
                        .child("按北京时间 UTC+8")
                        .into_any_element(),
                ),
                p,
            ))
            .child(
                div()
                    .p(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(9.0))
                    .child(chips)
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .gap(px(7.0))
                            .pt(px(9.0))
                            .border_t_1()
                            .border_color(rgb(p.line))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(p.text_2))
                                    .child("失败重试"),
                            )
                            .child(narrow_field(attempts, p))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(p.text_2))
                                    .child("次，间隔"),
                            )
                            .child(narrow_field(delay, p))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(p.text_2))
                                    .child("分钟"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(p.text_3))
                                    .child("1–10 次 · 1–60 分钟"),
                            ),
                    ),
            );

        if self.active == Some(Field::AddTime) {
            let input = self.text_field(Field::AddTime, true, false, p, cx);
            panel = panel.child(
                div()
                    .px(px(12.0))
                    .pb(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child("新增时间点"),
                    )
                    .child(narrow_field_wide(input, p))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child("回车确认"),
                    ),
            );
        }
        panel.into_any_element()
    }

    fn render_log_panel(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut lines = div()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .px(px(13.0))
            .py(px(10.0))
            .bg(rgb(p.surface_2));

        let start = self.state.logs.len().saturating_sub(40);
        if self.state.logs.is_empty() {
            lines = lines.child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_3))
                    .child("暂无日志，到点执行后会显示在这里"),
            );
        }
        for line in &self.state.logs[start..] {
            let color = match line.kind {
                LogKind::Info => p.text_2,
                LogKind::Ok => p.ok,
                LogKind::Warn => p.warn,
                LogKind::Bad => p.bad,
            };
            lines = lines.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child(line.time.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(11.5))
                            .text_color(rgb(color))
                            .child(line.text.clone()),
                    ),
            );
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(p.line))
            .bg(rgb(p.surface))
            .overflow_hidden()
            .child(panel_header(
                "运行日志",
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .id("copy-log")
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .text_color(rgb(p.accent))
                                .child("复制")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let text = this
                                        .state
                                        .logs
                                        .iter()
                                        .map(|line| format!("{}  {}", line.time, line.text))
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    if let Ok(mut clipboard) = arboard_set(&text) {
                                        this.state.log(LogKind::Info, "日志已复制到剪贴板");
                                        let _ = clipboard;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("clear-log")
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .text_color(rgb(p.text_3))
                                .child("清空")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.logs.clear();
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("open-log-dir")
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .text_color(rgb(p.text_3))
                                .child("打开目录")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let _ = open_url(&this.state.data_dir().display().to_string());
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                ),
                p,
            ))
            .child(lines)
            .into_any_element()
    }

    fn render_statusbar(&self, p: &Palette, cx: &mut Context<Self>, readonly: bool) -> AnyElement {
        let has_error = self.error.is_some();
        div()
            .h(px(STATUSBAR_H))
            .flex_none()
            .px(px(14.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(rgb(p.surface_2))
            .border_t_1()
            .border_color(rgb(p.line))
            .text_size(px(11.5))
            .text_color(rgb(p.text_2))
            .child(dot(
                if self.state.paused { p.warn } else { p.ok },
                !self.state.paused,
                p,
            ))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_2))
                    .child(self.state.status_line()),
            )
            .child(div().w(px(1.0)).h(px(14.0)).bg(rgb(p.line)))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_3))
                    .child("Ctrl + Enter 立即签到"),
            )
            .child(div().flex_1())
            .when_some(self.error.clone(), |d, message| {
                d.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.bad))
                        .child(message),
                )
            })
            .when(!has_error, |d| {
                d.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_3))
                        .child(self.state.save_hint()),
                )
            })
            .child(if readonly {
                button("删除账号", BtnKind::Danger, BtnSize::Small, false, p)
                    .id("delete-account")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.delete_selected();
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                button("保存", BtnKind::Primary, BtnSize::Small, !self.state.dirty, p)
                    .id("save")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_empty(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let step = |number: &'static str, title: &'static str, hint: &'static str| {
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .p(px(11.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgb(p.line))
                .bg(rgb(p.surface_2))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(18.0))
                                .h(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(9.0))
                                .bg(rgb(p.surface_3))
                                .text_size(px(10.5))
                                .text_color(rgb(p.text_2))
                                .child(number),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(p.text))
                                .child(title),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(p.text_3))
                        .child(hint),
                )
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(18.0))
                    .pt(px(14.0))
                    .pb(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .border_b_1()
                    .border_color(rgb(p.line))
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .text_color(rgb(p.text))
                                    .child("开始使用"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(p.text_3))
                                    .mt(px(1.0))
                                    .child("三步之后，每天由后台自动签到，关闭窗口也会继续运行"),
                            ),
                    )
                    .child(pill("未运行", p.text_2, p.surface_3, p.surface_3, true)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(520.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(img("logo.png").w(px(52.0)).h(px(52.0)))
                            .child(
                                div()
                                    .mt(px(14.0))
                                    .text_size(px(17.0))
                                    .text_color(rgb(p.text))
                                    .child("先添加一个账号"),
                            )
                            .child(
                                div()
                                    .mt(px(6.0))
                                    .text_size(px(12.5))
                                    .text_color(rgb(p.text_3))
                                    .text_center()
                                    .child(
                                        "用微信扫码登录教学平台，姓名与班级会自动读取；之后填一次地点和时间即可长期使用。",
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(16.0))
                                    .flex()
                                    .gap(px(10.0))
                                    .child(
                                        button("扫码添加账号", BtnKind::Primary, BtnSize::Normal, false, p)
                                            .id("empty-scan")
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.start_qr_login(window, cx);
                                            })),
                                    )
                                    .child(
                                        button("导入配置文件", BtnKind::Default, BtnSize::Normal, false, p)
                                            .id("import-config")
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.import_config();
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(22.0))
                                    .w_full()
                                    .flex()
                                    .gap(px(10.0))
                                    .child(div().flex_1().child(step("1", "扫码登录", "不用抓包，微信扫一次即可")))
                                    .child(div().flex_1().child(step("2", "地点与时间", "坐标拾取 + 多个时间点")))
                                    .child(div().flex_1().child(step("3", "交给后台", "关窗也继续跑，失败会重试"))),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(STATUSBAR_H))
                    .flex_none()
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .bg(rgb(p.surface_2))
                    .border_t_1()
                    .border_color(rgb(p.line))
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_2))
                    .child(dot(p.text_3, false, p))
                    .child(div().text_size(px(11.5)).child("未运行"))
                    .child(div().w(px(1.0)).h(px(14.0)).bg(rgb(p.line)))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child("添加账号后自动开始调度"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child(format!("数据目录 {}", self.state.data_dir().display())),
                    ),
            )
            .into_any_element()
    }

    /// 一个文本输入框：外框 + 文本 + 聚焦时的光标。
    fn text_field(
        &self,
        field: Field,
        mono: bool,
        readonly: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.active == Some(field);
        let value = self.value_of(field);
        let placeholder = match field {
            Field::Lat => "39.908720",
            Field::Lng => "116.397450",
            Field::Wx => "未配置",
            Field::Qq => "未配置",
            Field::RetryAttempts => "3",
            Field::RetryDelay => "5",
            Field::AddTime => "21:31",
        };
        let empty = value.trim().is_empty();
        let index = field.index();
        let handle = self.focus[index].clone();
        let raw = if mono {
            format!("{value}")
        } else {
            value.clone()
        };

        let text_color = if empty { p.text_3 } else { p.text };
        let shown = if empty {
            placeholder.to_string()
        } else {
            raw.clone()
        };
        let size = if mono { 12.5 } else { 13.0 };

        let mut element = div()
            .id(("field", index))
            .flex()
            .items_center()
            .w_full()
            .min_w_0();

        let mut frame = input_frame(focused, readonly, false, p)
            .w_full()
            .flex()
            .items_center()
            .gap(px(1.0))
            .child(
                div()
                    .text_size(px(size))
                    .text_color(rgb(text_color))
                    .child(shown),
            );
        if focused && !readonly {
            frame = frame.child(div().w(px(1.0)).h(px(14.0)).bg(rgb(p.accent)));
        }
        element = element.child(frame);

        if readonly {
            return element.into_any_element();
        }

        element
            .track_focus(&handle)
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.active = Some(field);
                    window.focus(&this.focus[index], cx);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                this.on_key(field, event, cx);
            }))
            .into_any_element()
    }
}

/// 窗口按钮：46×36 直角，悬停变浅，关闭变红。
///
/// 字形用等宽符号（─ □ ✕）而不是描出来的矢量：这个版本没有 `rotate`，
/// 10px 下两者观感一致，且符号能跟着主题换色。
fn window_button(area: WindowControlArea, p: &Palette) -> AnyElement {
    let danger = area == WindowControlArea::Close;
    let glyph = match area {
        WindowControlArea::Min => "─",
        WindowControlArea::Max => "□",
        _ => "✕",
    };
    let hover_fg = if danger { 0xffffff } else { p.text };

    div()
        .w(px(WINDOW_BTN_W))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.0))
        .text_color(rgb(p.text_2))
        .window_control_area(area)
        .hover(move |style| {
            if danger {
                style.bg(rgb(p.close_hover)).text_color(rgb(hover_fg))
            } else {
                style.bg(rgb(p.surface_3)).text_color(rgb(hover_fg))
            }
        })
        .child(glyph)
        .into_any_element()
}

/// 标签 + 输入框的一列。
fn field_block(title: impl Into<SharedString>, input: AnyElement, p: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(p.text_2))
                .child(title.into()),
        )
        .child(input)
        .into_any_element()
}

/// 标签前带开关的输入框列。
fn field_block_with_switch(
    title: &'static str,
    checked: bool,
    input: AnyElement,
    p: &Palette,
    _readonly: bool,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .flex_1()
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(switch(checked, false, p))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(p.text_2))
                        .child(title),
                ),
        )
        .child(input)
        .into_any_element()
}

/// 窄输入框（重试次数 / 间隔）。
fn narrow_field(input: AnyElement, _p: &Palette) -> AnyElement {
    div().w(px(56.0)).flex_none().child(input).into_any_element()
}

fn narrow_field_wide(input: AnyElement, _p: &Palette) -> AnyElement {
    div().w(px(96.0)).flex_none().child(input).into_any_element()
}

/// 起一个不弹黑窗口的子进程。
///
/// `cmd` 默认会分配新的控制台窗口，点一下「打开目录」就闪一个黑框，
/// 必须显式带 `CREATE_NO_WINDOW`。所有外部命令都走这里。
pub(crate) fn hidden_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// 打开一个 URL 或目录（资源管理器 / 浏览器）。
pub(crate) fn open_url(target: &str) -> std::io::Result<()> {
    hidden_command("cmd")
        .args(["/C", "start", "", target])
        .spawn()
        .map(|_| ())
}

/// 复制到剪贴板（走 PowerShell，避免额外依赖）。
fn arboard_set(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = hidden_command("powershell")
        .args(["-NoProfile", "-Command", "$input | Set-Clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    Ok(())
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let mut root = div()
            .v_flex()
            .size_full()
            .bg(rgb(p.desk))
            .text_size(px(13.0))
            .text_color(rgb(p.text))
            .child(self.render_titlebar(p))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.render_sidebar(p, cx))
                    .child(self.render_main(p, cx)),
            );

        match self.overlay {
            Overlay::CloseHint => root = root.child(self.render_close_hint(p, cx)),
            Overlay::Qr => root = root.child(crate::ui::qr_modal::render(self, p, cx)),
            Overlay::Settings(_) => root = root.child(crate::ui::settings::render(self, p, cx)),
            Overlay::None => {}
        }
        root
    }
}

impl AppView {
    fn render_close_hint(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .bottom(px(14.0))
            .right(px(14.0))
            .w(px(300.0))
            .p(px(11.0))
            .flex()
            .gap(px(9.0))
            .rounded(px(10.0))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .child(img("logo.png").w(px(18.0)).h(px(18.0)).mt(px(1.0)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(p.text))
                            .child("已最小化到托盘"),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_3))
                            .child("定时签到仍在后台运行。要退出请右键托盘图标。"),
                    )
                    .child(
                        div()
                            .mt(px(9.0))
                            .flex()
                            .gap(px(6.0))
                            .child(
                                button("打开主窗口", BtnKind::Primary, BtnSize::Xs, false, p)
                                    .id("hint-open")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.overlay = Overlay::None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                button("不再提示", BtnKind::Default, BtnSize::Xs, false, p)
                                    .id("hint-mute")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_hint_muted = true;
                                        this.overlay = Overlay::None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
