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

use gpui_kit::component::scroll::ScrollableElement;

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
    /// 添加执行时间：从列表点选，不手输
    TimePicker,
}

/// 当前展开了哪个下拉列表（同一时刻只允许一个）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenSelect {
    None,
    RetryAttempts,
    RetryDelay,
    /// 时间选择器里的小时列表
    Hour,
    /// 时间选择器里的分钟列表
    Minute,
}

/// 重试间隔的常用值（分钟），避免 60 个选项把列表拉太长。
pub const RETRY_DELAYS: [u32; 10] = [1, 2, 3, 5, 10, 15, 20, 30, 45, 60];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
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
    /// 时间选择器里当前选中的时 / 分
    pub picker_hour: u32,
    pub picker_minute: u32,
    /// 当前展开的下拉（重试次数 / 重试间隔 / 时 / 分）
    pub open_select: OpenSelect,
    /// 设置窗口的句柄：有了它就说明设置已经开着（同一时刻只允许一个）。
    pub settings_window: Option<WindowId>,
    /// 展开中的下拉浮层的左上角（窗口坐标）。
    ///
    /// 列表必须画在根层：主界面 body 和弹窗都是 overflow_hidden，
    /// 放在原地展开会把窗口撑高，或者被裁掉一半。
    pub dropdown_at: Option<(OpenSelect, Pixels, Pixels)>,
    /// 日志区的滚动位置（日志多了要能翻）
    pub log_scroll: ScrollHandle,
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
        // 时间选择器的初始值跟着现有时间走，打开就能少点几下
        let (picker_hour, picker_minute) =
            crate::ui::time_picker::default_pick(&state.settings.times);
        let mut overlay = Overlay::None;
        // 截图/QA：AUTOSIGN_OPEN=settings|data|qr|time 直接打开对应覆盖层
        if let Ok(value) = std::env::var("AUTOSIGN_OPEN") {
            overlay = match value.as_str() {
                "settings" => Overlay::None,
                "data" => Overlay::None,
                "qr" => Overlay::Qr,
                "time" => Overlay::TimePicker,
                _ => Overlay::None,
            };
        }

        // QA 回归用：AUTOSIGN_TEST_DIALOG=import|export 启动 2 秒后自动弹文件对话框。
        // 目的是能脚本化验证「对话框打开时主窗口仍然响应」（卡死是历史 bug）。
        if let Ok(kind) = std::env::var("AUTOSIGN_TEST_DIALOG") {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    if kind == "export" {
                        view.export_config(cx);
                    } else {
                        view.import_config(cx);
                    }
                });
            })
            .detach();
        }

        // QA 回归用：AUTOSIGN_TEST_TOGGLE=wx|qq 启动 3 秒后切换对应通知渠道，
        // 用来验证「关掉后输入框是否真的消失」。
        if let Ok(which) = std::env::var("AUTOSIGN_TEST_TOGGLE") {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    let index = view.state.selected;
                    let is_wx = which == "wx";
                    if let Some(student) = view.state.config.students.get_mut(index) {
                        let next = if is_wx {
                            !student.wx_enabled()
                        } else {
                            !student.qq_enabled()
                        };
                        if is_wx {
                            student.wx_on = Some(next);
                        } else {
                            student.qq_on = Some(next);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }

        // QA 回归用：AUTOSIGN_TEST_HIDE=1 先隐藏窗口、3 秒后再唤回，
        // 用来验证「最小化到托盘后还能不能找回来 / 关闭是否真的生效」。
        if std::env::var("AUTOSIGN_TEST_HIDE").is_ok() {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                crate::system::win::hide_main_window();
                eprintln!("QA: hidden");
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                crate::system::win::show_main_window();
                eprintln!("QA: shown");
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }

        // QA 回归用：AUTOSIGN_TEST_RUNNOW=1 启动 3 秒后自动触发「立即签到」，
        // 用来验证调度线程真的执行了一轮（不是只写了一行日志）。
        if std::env::var("AUTOSIGN_TEST_RUNNOW").is_ok() {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.run_now();
                    cx.notify();
                });
            })
            .detach();
        }

        // QA 回归用：AUTOSIGN_TEST_COPY=1 启动 2 秒后自动复制日志，
        // 用来验证中文不再乱码（外部用 Get-Clipboard 读回比对）。
        if std::env::var("AUTOSIGN_TEST_COPY").is_ok() {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.copy_logs();
                    cx.notify();
                });
            })
            .detach();
        }

        let mut view = Self {
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
            picker_hour,
            picker_minute,
            open_select: OpenSelect::None,
            dropdown_at: None,
            settings_window: None,
            log_scroll: ScrollHandle::new(),
            focus,
            close_hint_muted: false,
            close_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        // 截图/QA：AUTOSIGN_PICKER=hour|minute|attempts|delay 直接把对应下拉展开
        if let Ok(kind) = std::env::var("AUTOSIGN_PICKER") {
            let selected = match kind.as_str() {
                "hour" => OpenSelect::Hour,
                "minute" => OpenSelect::Minute,
                "attempts" => OpenSelect::RetryAttempts,
                "delay" => OpenSelect::RetryDelay,
                _ => OpenSelect::None,
            };
            view.open_select = selected;
            if selected != OpenSelect::None {
                // 浮层需要位置，这里给一个固定点用于截图
                view.dropdown_at = Some((selected, px(430.0), px(430.0)));
            }
        }
        // QA：等实体构造完成再开设置窗口，否则会撞上「entity 正在被更新」
        if let Ok(open) = std::env::var("AUTOSIGN_OPEN") {
            let want_data = open == "data";
            if open == "settings" || want_data {
                if want_data {
                    view.settings_tab = SettingsTab::Data;
                }
                view.open_settings(cx);
            }
        }
        view
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
                        view.tick(cx);
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
    pub fn tick(&mut self, cx: &mut Context<Self>) {
        let (running, paused, next, last_exit) = self.runtime.snapshot();
        self.state.running = running;
        self.state.paused = paused;
        self.state.next_run_at = next;
        self.state.last_exit = last_exit;
        let before = self.state.logs.len();
        for (kind, line) in self.runtime.drain() {
            self.state.log(kind, line);
        }
        if self.state.logs.len() != before {
            // 有新日志就跟着滚到底，不然用户得自己往下翻
            self.log_scroll.scroll_to_bottom();
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
                // 提示必须开独立窗口：主窗口这时候已经隐藏，画在里面等于没提示
                if !crate::ui::close_hint::is_muted() {
                    crate::ui::close_hint::open(cx, self.dark);
                }
            } else {
                std::process::exit(0);
            }
        }

        // 托盘「打开主窗口」/「设置」
        if self.runtime.take_show_request() {
            crate::system::win::show_main_window();
            let want_settings = self
                .runtime
                .shared
                .lock()
                .map(|mut guard| std::mem::take(&mut guard.show_settings))
                .unwrap_or(false);
            if want_settings {
                self.open_settings(cx);
            }
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
        match field {
            Field::AddTime => {
                self.add_time = value;
                return;
            }
            _ => {}
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

        // Ctrl+V 粘贴：gpui 只给按键，剪贴板要自己读
        if event.keystroke.modifiers.control {
            match key {
                "v" => {
                    if let Some(text) = crate::system::win::clipboard_text() {
                        // 单行输入框：换行与制表符直接去掉
                        let cleaned: String = text
                            .chars()
                            .filter(|c| *c != '\r' && *c != '\n' && *c != '\t')
                            .collect();
                        value.push_str(&cleaned);
                        self.set_value(field, value);
                    }
                    cx.notify();
                    return;
                }
                "a" => {
                    // 全选后再输入即替换：这里退化成清空，符合单行输入框的直觉
                    self.set_value(field, String::new());
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

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

    /// 打开设置窗口（独立窗口：可拖动、可最小化，不再挤在主界面里当覆盖层）。
    ///
    /// 开窗必须放到事件回调之外：`open_window` 会立刻渲染这个 View，
    /// 而此时 entity 正在被 update —— 直接调用会 panic（already being updated）。
    /// 所以统一走「spawn 一次 + AsyncApp::update」，在借用释放后再开窗。
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.settings_window {
            if let Some(handle) = cx
                .windows()
                .into_iter()
                .find(|handle| handle.window_id() == id)
            {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                return;
            }
            self.settings_window = None;
        }

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(260.0), px(180.0)),
                size: size(px(860.0), px(560.0)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("设置".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_min_size: Some(size(px(720.0), px(460.0))),
            ..Default::default()
        };

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1))
                .await;
            let Some(entity) = this.upgrade() else {
                return;
            };
            let opened = cx.update(|app| app.open_window(options, move |_, _| entity.clone()));
            if let Ok(handle) = opened {
                let id = handle.window_id();
                let _ = this.update(cx, |view, cx| {
                    view.settings_window = Some(id);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// 关闭设置窗口（设置页里的「完成」与标题栏 ✕ 都走这里）。
    pub fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_window = None;
        window.remove_window();
        cx.notify();
    }

    /// 立即签到（按钮与 Ctrl+Enter 共用）。
    pub fn run_now(&mut self) {
        self.runtime.request_run_now();
        self.state.log(LogKind::Info, "已请求立即签到");
    }

    /// 把当前日志复制到剪贴板（Win32 Unicode，中文不会乱码）。
    pub fn copy_logs(&mut self) {
        let text = self
            .state
            .logs
            .iter()
            .map(|line| format!("{}  {}", line.time, line.text))
            .collect::<Vec<_>>()
            .join("\r\n");
        if text.is_empty() {
            return;
        }
        match set_clipboard_text(&text) {
            Ok(()) => self.state.log(LogKind::Info, "日志已复制到剪贴板"),
            Err(_) => self
                .state
                .log(LogKind::Bad, "复制失败：剪贴板被其它程序占用"),
        }
    }

    /// 清空界面日志，同时把今天的日志文件清掉（否则重启又回来了）。
    pub fn clear_logs(&mut self) {
        self.state.logs.clear();
        let path = self.state.log_file();
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path);
    }

    /// 只写渠道开关状态（不改 Key）。
    fn set_channel(&mut self, field: Field, on: bool) {
        let index = self.state.selected;
        if let Some(student) = self.state.config.students.get_mut(index) {
            if field == Field::Wx {
                student.wx_on = Some(on);
            } else {
                student.qq_on = Some(on);
            }
        }
        let _ = self.state.config.save(self.state.paths.config_file());
    }

    /// 关闭态点「编辑」：打开渠道并聚焦输入框。
    fn open_channel(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        self.set_channel(field, true);
        window.focus(&self.focus[field.index()], cx);
        cx.notify();
    }

    /// 点通知开关：只切换「渠道是否启用」，输入框跟着显示/隐藏，Key 一律保留。
    fn toggle_notify(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.state.selected;
        let Some(student) = self.state.config.students.get(index) else {
            return;
        };
        let is_wx = field == Field::Wx;
        let next = if is_wx {
            !student.wx_enabled()
        } else {
            !student.qq_enabled()
        };

        self.set_channel(field, next);

        let name = if is_wx { "微信通知" } else { "QQ 通知" };
        if next {
            window.focus(&self.focus[field.index()], cx);
            self.state
                .log(LogKind::Info, format!("已开启{name}，填入 Key 后即可推送"));
        } else {
            self.state.log(
                LogKind::Info,
                format!("已关闭{name}，Key 仍保留在 data.json 里"),
            );
        }
        // 少了这一句，状态改了但界面不刷新，表现出来就是「开关点不动、输入框不消失」
        cx.notify();
    }

    /// 导出到指定路径。（文件对话框回来后调用；也是单测入口）
    pub fn apply_export(&mut self, path: &std::path::Path) {
        match crate::ui::transfer::write_bundle_file(path, &self.state.config, &self.state.settings)
        {
            Ok(()) => self.state.log(
                LogKind::Ok,
                format!("已导出配置到 {}", path.display()),
            ),
            Err(error) => self
                .state
                .log(LogKind::Bad, format!("导出失败：{error}")),
        }
    }

    /// 从指定文件导入。（文件对话框回来后调用；也是单测入口）
    pub fn apply_import(&mut self, path: &std::path::Path) {
        let (config, settings) = match crate::ui::transfer::read_bundle_file(path) {
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
        self.error = None;
        self.state.log(LogKind::Ok, format!("已导入配置：{summary}"));
    }

    /// 导出配置：账号 + 设置，不含日志。
    ///
    /// 对话框必须走 async 版本：`rfd::FileDialog` 是同步模态，它会在**调用线程**上
    /// 起自己的消息循环，而这个线程正是 GPUI 的渲染线程 —— 一旦进来，
    /// gpui 的窗口过程会重入到已借用的 App 状态，整程序卡死（任务管理器都结束不掉）。
    /// `AsyncFileDialog` 把对话框放到 rfd 自己开的线程里，界面照常响应。
    pub fn export_config(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_title("导出配置")
                .set_file_name("AutoSignBJMF-配置.json")
                .add_filter("JSON", &["json"])
                .save_file()
                .await
            else {
                return;
            };
            let path = file.path().to_path_buf();
            let _ = this.update(cx, |view, cx| {
                view.apply_export(&path);
                cx.notify();
            });
        })
        .detach();
    }

    /// 导入配置：支持本程序导出格式，也支持直接选 Python 版的 data.json。
    pub fn import_config(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_title("导入配置")
                .add_filter("JSON", &["json"])
                .pick_file()
                .await
            else {
                return;
            };
            let path = file.path().to_path_buf();
            let _ = this.update(cx, |view, cx| {
                view.apply_import(&path);
                cx.notify();
            });
        })
        .detach();
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

    fn render_titlebar(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, window, _| window.start_window_move()),
                    )
                    .child(img("logo-64.png").w(px(20.0)).h(px(20.0)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(p.text_2))
                            .child("班级魔方自动签到"),
                    )
                    .child(pill(text, fg, bg, border, true)),
            )
            .child(window_button(WindowButton::Min, p, cx))
            .child(window_button(WindowButton::Max, p, cx))
            .child(window_button(WindowButton::Close, p, cx))
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
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            // 扫码是唯一能拿到凭证的方式；手动只建条目，之后仍要扫码
                            .child(
                                button("＋ 添加账号", BtnKind::Default, BtnSize::Xs, false, p)
                                    .id("add-account")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.start_qr_login(window, cx);
                                    })),
                            )
                            ,
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
                                // 和设置页保持一致：改完就落盘，否则重启又变回去
                                let _ = this
                                    .state
                                    .settings
                                    .save(this.state.paths.settings_file());
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
                                let _ = this
                                    .state
                                    .settings
                                    .save(this.state.paths.settings_file());
                                cx.notify();
                            }))
                            .child(switch(minimize, false, p)),
                    ),
            )
            // 一键暂停 / 恢复全部账号
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(1.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(p.text_2))
                            .child("全部账号"),
                    )
                    .child(
                        button(
                            if self.state.paused { "恢复" } else { "暂停" },
                            BtnKind::Default,
                            BtnSize::Xs,
                            false,
                            p,
                        )
                        .id("pause-all")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let next = !this.state.paused;
                            this.runtime.set_paused(next);
                            this.state.paused = next;
                            this.state.log(
                                LogKind::Info,
                                if next {
                                    "已暂停全部账号（单个账号仍可单独恢复）".to_string()
                                } else {
                                    "已恢复全部账号".to_string()
                                },
                            );
                            cx.notify();
                        })),
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
                                this.open_settings(cx);
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

        // 暂停是「按账号」的：这个账号被暂停时，pill 直接显示「已暂停」
        let account_paused = self.runtime.account_paused(index);
        let (pill_label, pill_fg, pill_bg, pill_border) = if account_paused {
            ("已暂停", p.text_2, p.surface_3, p.surface_3)
        } else {
            let colors = match status {
                AccountStatus::Running => (p.ok, p.ok_soft, p.ok_line),
                AccountStatus::NeedsLogin | AccountStatus::Failed => {
                    (p.warn, p.warn_soft, p.warn_line)
                }
                _ => (p.text_2, p.surface_3, p.surface_3),
            };
            (status.label(), colors.0, colors.1, colors.2)
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
            .child(pill(pill_label, pill_fg, pill_bg, pill_border, true));

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
                    // 只针对当前选中的账号；「全部」在左侧底部
                    button(
                        if account_paused { "恢复此账号" } else { "暂停此账号" },
                        BtnKind::Default,
                        BtnSize::Small,
                        false,
                        p,
                    )
                        .id("toggle-pause")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let index = this.state.selected;
                            let next = !this.runtime.account_paused(index);
                            this.runtime.set_account_paused(index, next);
                            let who = this
                                .state
                                .students()
                                .get(index)
                                .map(|student| student.name.clone())
                                .unwrap_or_else(|| format!("账号{}", index + 1));
                            this.state.log(
                                LogKind::Info,
                                if next {
                                    format!("已暂停「{who}」，其它账号照常执行")
                                } else {
                                    format!("已恢复「{who}」")
                                },
                            );
                            cx.notify();
                        })),
                )
                .child(
                    button("立即签到", BtnKind::Primary, BtnSize::Small, false, p)
                        .id("run-now")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_now();
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
        // 渠道关掉时输入框整个不渲染，只留一行说明；Key 仍留在 data.json 里
        let (wx_on, qq_on) = self
            .state
            .selected_student()
            .map(|student| (student.wx_enabled(), student.qq_enabled()))
            .unwrap_or((false, false));
        let wx = if wx_on {
            // 通知 Key 与签到凭证无关，账号需要重登时也应该能编辑
            self.text_field(Field::Wx, true, false, p, cx)
        } else {
            closed_channel_hint(Field::Wx, p, cx)
        };
        let qq = if qq_on {
            self.text_field(Field::Qq, true, false, p, cx)
        } else {
            closed_channel_hint(Field::Qq, p, cx)
        };

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
                                notify_header(
                                    "微信通知 · Server 酱",
                                    wx_on,
                                    Field::Wx,
                                    false,
                                    p,
                                    cx,
                                ),
                                wx,
                            ))
                            .child(field_block_with_switch(
                                notify_header(
                                    "QQ 通知 · Qmsg 酱",
                                    qq_on,
                                    Field::Qq,
                                    false,
                                    p,
                                    cx,
                                ),
                                qq,
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
                .on_click(cx.listener(|this, _, _window, cx| {
                    // 不再让用户手打 21:31:00，直接弹时间列表点选
                    let (hour, minute) =
                        crate::ui::time_picker::default_pick(&this.state.settings.times);
                    this.picker_hour = hour;
                    this.picker_minute = minute;
                    this.open_select = OpenSelect::None;
                    this.overlay = Overlay::TimePicker;
                    cx.notify();
                }))
                .into_any_element()
        });

        // 重试次数与间隔都改成下拉点选，不再手输数字
        let attempts_value = self.state.settings.retry_attempts;
        let delay_value = self.state.settings.retry_delay_minutes;

        let attempts = select_trigger(
            format!("{attempts_value}"),
            self.open_select == OpenSelect::RetryAttempts,
            62.0,
            "retry-attempts",
            readonly,
            p,
            cx,
            move |this| {
                this.open_select = if this.open_select == OpenSelect::RetryAttempts {
                    OpenSelect::None
                } else {
                    OpenSelect::RetryAttempts
                };
            },
        );
        let delay = select_trigger(
            format!("{delay_value}"),
            self.open_select == OpenSelect::RetryDelay,
            62.0,
            "retry-delay",
            readonly,
            p,
            cx,
            move |this| {
                this.open_select = if this.open_select == OpenSelect::RetryDelay {
                    OpenSelect::None
                } else {
                    OpenSelect::RetryDelay
                };
            },
        );

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
                            .flex_col()
                            .gap(px(6.0))
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
                                    .child(attempts)
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(p.text_2))
                                            .child("次，间隔"),
                                    )
                                    .child(delay)
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
                            )

                    ),
            );

        // 注：时间点不再用文本输入（见 ui/time_picker.rs），这里不再渲染「新增时间点」行。
        panel.into_any_element()
    }

    fn render_log_panel(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut lines = div()
            .id("log-scroll")
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.log_scroll)
            .px(px(13.0))
            .py(px(10.0))
            .bg(rgb(p.surface_2));

        // 保留最近 200 条，超出的靠滚动看
        let start = self.state.logs.len().saturating_sub(200);
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
                                    this.copy_logs();
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
                                    this.clear_logs();
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
                            .child(img("logo-64.png").w(px(52.0)).h(px(52.0)))
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
                                                this.import_config(cx);
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
/// 标题栏按钮的三种动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Min,
    Max,
    Close,
}

/// 标题栏的最小化 / 最大化 / 关闭。
///
/// 之前用 `window_control_area`，那是把命中判定交给系统的 WM_NCHITTEST ——
/// 鼠标在标题栏移动时系统会不停地问「这点是什么区域」，叠上 hover 重绘就明显卡顿
/// （设置窗口用的是系统标题栏，所以没事）。改成普通点击 + 显式 API。
fn window_button(kind: WindowButton, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let (glyph, danger) = match kind {
        WindowButton::Min => ("─", false),
        WindowButton::Max => ("□", false),
        WindowButton::Close => ("✕", true),
    };
    let hover_fg = if danger { 0xffffff } else { p.text };

    div()
        .id(match kind {
            WindowButton::Min => "win-min",
            WindowButton::Max => "win-max",
            WindowButton::Close => "win-close",
        })
        .w(px(WINDOW_BTN_W))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.0))
        .text_color(rgb(p.text_2))
        .cursor_pointer()
        .hover(move |style| {
            if danger {
                style.bg(rgb(p.close_hover)).text_color(rgb(hover_fg))
            } else {
                style.bg(rgb(p.surface_3)).text_color(rgb(hover_fg))
            }
        })
        .on_click(cx.listener(move |this, _, window, cx| match kind {
            WindowButton::Min => window.minimize_window(),
            WindowButton::Max => window.zoom_window(),
            WindowButton::Close => {
                // 关闭 = 交给原来的「最小化到托盘」流程，别直接销毁窗口
                this.close_signal
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                cx.notify();
            }
        }))
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
/// 渠道关闭时代替输入框的一行说明，高度与输入框一致，避免两列错位。
/// 渠道关闭时不再画一个「假输入框」，只留一行状态 + 一个编辑按钮。
/// 点编辑 = 打开该渠道并聚焦输入框。
fn closed_channel_hint(
    field: Field,
    p: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let label = if field == Field::Wx { "微信" } else { "QQ" };
    div()
        .h(px(30.0))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgb(p.line))
        .bg(rgb(p.surface_2))
        .child(
            div()
                .flex_1()
                .text_size(px(11.5))
                .text_color(rgb(p.text_3))
                .child(format!("{label}通知已关闭，不发送")),
        )
        .child(
            div()
                .id(SharedString::from(match field {
                    Field::Wx => "edit-wx",
                    _ => "edit-qq",
                }))
                .px(px(8.0))
                .py(px(2.0))
                .rounded(px(5.0))
                .text_size(px(11.0))
                .text_color(rgb(p.accent))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(p.surface_3)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_channel(field, window, cx);
                }))
                .child("编辑"),
        )
        .into_any_element()
}

/// 通知渠道的标题行：开关 + 名称，整行可点。
///
/// 开关状态就是「Key 是否非空」，所以点它等于：已配置 → 清空 Key（关掉渠道）；
/// 未配置 → 聚焦输入框，让用户去填。
fn notify_header(
    title: &'static str,
    checked: bool,
    field: Field,
    readonly: bool,
    p: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let mut header = div()
        .id(SharedString::from(format!("notify-switch-{title}")))
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(switch(checked, readonly, p))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(p.text_2))
                .child(title),
        );

    if !readonly {
        header = header
            .cursor_pointer()
            .hover(|style| style.opacity(0.8))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.toggle_notify(field, window, cx);
            }));
    }
    header.into_any_element()
}

/// 带开关标题的输入块。标题行由调用方构造（开关要能点）。
fn field_block_with_switch(header: AnyElement, input: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .flex_1()
        .min_w_0()
        .child(header)
        .child(input)
        .into_any_element()
}

/// 根据当前展开的下拉，构造浮层里的选项列表。
fn dropdown_overlay(
    kind: OpenSelect,
    view: &mut AppView,
    p: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let (items, width): (Vec<(String, bool)>, f32) = match kind {
        OpenSelect::RetryAttempts => {
            let current = view.state.settings.retry_attempts;
            (
                (1..=10u32)
                    .map(|n| (format!("{n} 次"), n == current))
                    .collect(),
                132.0,
            )
        }
        OpenSelect::RetryDelay => {
            let current = view.state.settings.retry_delay_minutes;
            (
                RETRY_DELAYS
                    .iter()
                    .map(|n| (format!("{n} 分钟"), *n == current))
                    .collect(),
                132.0,
            )
        }
        OpenSelect::Hour => {
            let current = view.picker_hour;
            (
                (0..24u32)
                    .map(|h| (format!("{h:02} 时"), h == current))
                    .collect(),
                132.0,
            )
        }
        OpenSelect::Minute => {
            let current = view.picker_minute;
            (
                (0..(60 / 5))
                    .map(|index| {
                        let m = index * 5;
                        (format!("{m:02} 分"), m == current)
                    })
                    .collect(),
                132.0,
            )
        }
        OpenSelect::None => return div().into_any_element(),
    };

    dropdown_list(items, width, 2, "overlay-list", p, cx, move |this, index| {
        match kind {
            OpenSelect::RetryAttempts => {
                let _ = this.state.set_retry_attempts(index as u32 + 1);
            }
            OpenSelect::RetryDelay => {
                let _ = this.state.set_retry_delay(RETRY_DELAYS[index]);
            }
            OpenSelect::Hour => this.picker_hour = index as u32,
            OpenSelect::Minute => this.picker_minute = index as u32 * 5,
            OpenSelect::None => {}
        }
        this.open_select = OpenSelect::None;
        this.dropdown_at = None;
    })
}

/// 下拉列表：竖直选项，项多时分成几列一次显示，不靠滚动。
///
/// 之前试过「限高 + 滚动 + 打开时滚到当前值」，gpui 的 ScrollHandle 要等布局完成
/// 才认目标项，时机不稳；直接按列铺开更可靠 —— 打开就能看到当前值。
pub fn dropdown_list(
    items: Vec<(String, bool)>,
    width: f32,
    _columns: usize,
    prefix: &'static str,
    p: &Palette,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView, usize) + 'static,
) -> AnyElement {
    let on_pick = std::rc::Rc::new(on_pick);

    // 单列：超出高度就滚动（用户明确要「一列下拉」，不要多列）
    {
        let mut column = div().flex().flex_col().flex_none().w_full();
        for (index, (label, selected)) in items.iter().enumerate() {
            let callback = on_pick.clone();
            let base = div()
                .id(SharedString::from(format!("{prefix}-item-{index}")))
                .h(px(26.0))
                .w_full()
                .flex_none()
                .flex()
                .items_center()
                .px(px(8.0))
                .rounded(px(5.0))
                .text_size(px(12.0))
                .child(label.clone());

            column = column.child(if *selected {
                base.bg(rgb(p.accent))
                    .text_color(rgb(0xffffff))
                    .into_any_element()
            } else {
                base.text_color(rgb(p.text_2))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_3)).text_color(rgb(p.text)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        callback(this, index);
                        cx.notify();
                    }))
                    .into_any_element()
            });
        }

        div()
            .id(SharedString::from(format!("{prefix}-list")))
            .w(px(width))
            .flex_none()
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(p.line_2))
            .bg(rgb(p.surface))
            // 选中态是实底，不裁剪的话会盖住圆角
            .overflow_hidden()
            .p(px(4.0))
            .child(column.max_h(px(232.0)).overflow_y_scrollbar())
            .into_any_element()
    }
}

/// 下拉触发器：外观像输入框，右侧一个 ▼，点开后在下方展开一列选项。
fn select_trigger(
    value: String,
    open: bool,
    width: f32,
    id: &'static str,
    readonly: bool,
    p: &Palette,
    cx: &mut Context<AppView>,
    on_toggle: impl Fn(&mut AppView) + 'static,
) -> AnyElement {
    let mut trigger = div()
        .id(SharedString::from(format!("select-{id}")))
        .w(px(width))
        .h(px(30.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.0))
        .px(px(8.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgb(if open { p.accent } else { p.line_2 }))
        .bg(rgb(p.surface))
        .text_size(px(12.0))
        .text_color(rgb(p.text))
        .child(div().flex_1().child(value))
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(p.text_3))
                .child(if open { "▲" } else { "▼" }),
        );

    if !readonly {
        trigger = trigger
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(p.text_3)))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                on_toggle(this);
                let pos = event.position();
                this.dropdown_at = if this.open_select == OpenSelect::None {
                    None
                } else {
                    Some((this.open_select, pos.x, pos.y))
                };
                cx.notify();
            }));
    }
    trigger.into_any_element()
}

/// 窄输入框（保留给以后可能的手输场景）。
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

/// 复制文本到剪贴板（Win32 直写 CF_UNICODETEXT）。
///
/// 之前走 `powershell -Command "$input | Set-Clipboard"`，中文会乱码：
/// PowerShell 从 stdin 读文本时按控制台的 OEM/ANSI 代码页解码，而我们写进去的是 UTF-8。
/// 直接用 Unicode 剪贴板格式既没有编码问题，也不用起子进程。
fn set_clipboard_text(text: &str) -> std::io::Result<()> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    // 以 0 结尾的 UTF-16，字节数而不是字符数
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        EmptyClipboard();
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            CloseClipboard();
            return Err(std::io::Error::last_os_error());
        }
        let target = GlobalLock(handle) as *mut u16;
        if target.is_null() {
            CloseClipboard();
            return Err(std::io::Error::last_os_error());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
        GlobalUnlock(handle);
        // 成功后内存归系统所有，不能自己释放
        if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
            CloseClipboard();
            return Err(std::io::Error::last_os_error());
        }
        CloseClipboard();
    }
    Ok(())
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();

        // 同一个 entity 在两个窗口里渲染：靠窗口句柄区分画主界面还是设置页
        if self.settings_window == Some(window.window_handle().window_id()) {
            return crate::ui::settings::render(self, p, cx);
        }

        let mut root = div()
            .v_flex()
            .size_full()
            .bg(rgb(p.desk))
            .text_size(px(13.0))
            .text_color(rgb(p.text))
            // 状态栏写着 Ctrl+Enter 立即签到，这里把它真正接上
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.modifiers.control && event.keystroke.key == "enter" {
                    this.run_now();
                    cx.notify();
                }
            }))
            .child(self.render_titlebar(p, cx))
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
            // 设置与「已最小化」提示都改成独立窗口了，这里只保留枚举值做兼容
            Overlay::Settings(_) | Overlay::CloseHint => {}
            Overlay::Qr => root = root.child(crate::ui::qr_modal::render(self, p, cx)),
            Overlay::TimePicker => root = root.child(crate::ui::time_picker::render(self, p, cx)),
            Overlay::None => {}
        }

        // 下拉浮层画在最上层：底下垫一个透明遮罩，点别处收起
        if let Some((kind, x, y)) = self.dropdown_at {
            root = root
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.dropdown_at = None;
                                this.open_select = OpenSelect::None;
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .left(x)
                        .top(y + px(4.0))
                        .child(dropdown_overlay(kind, self, p, cx)),
                );
        }
        root.into_any_element()
    }
}

impl AppView {
}
