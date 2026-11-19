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
use gpui_kit::assets::IconNamed;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::IndexPath;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::IconName;

use crate::state::{mask_key, AccountStatus, AppState, LogKind, PillTone};
use crate::theme::*;
use crate::ui::widgets::*;

/// 需要读写草稿的字段。
///
/// 重试次数/间隔/执行时间已经不是「输入框」了（改成了下拉与时间选择器），
/// 所以这里只剩四个真正靠文本编辑的字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Lat,
    Lng,
    Wx,
    Qq,
}


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
    /// 重试次数下拉（1–10）
    pub retry_attempts_select: Entity<SelectState<Vec<SharedString>>>,
    /// 重试间隔下拉（1–60 分钟）
    pub retry_delay_select: Entity<SelectState<Vec<SharedString>>>,
    /// 设置窗口的句柄：有了它就说明设置已经开着（同一时刻只允许一个）。
    pub settings_window: Option<WindowId>,
    /// 正在开窗中：双击「设置」时不至于开出两个窗口
    pub settings_opening: bool,
    /// 主窗口的句柄：同一个 entity 会在两个窗口里渲染，靠它区分画哪个界面。
    pub main_window_id: Option<WindowId>,
    /// 主窗口当前是否被藏到了托盘（主题刷新后要恢复原样）。
    pub main_hidden: bool,
    /// 纬度 / 经度 / 微信 Key / QQ Key：交给组件库的 Input 管编辑与光标
    pub lat_input: Entity<InputState>,
    pub lng_input: Entity<InputState>,
    pub wx_input: Entity<InputState>,
    pub qq_input: Entity<InputState>,
    /// 上一次同步到输入框的草稿版本号
    inputs_synced: Option<u64>,
    /// 两个窗口各自的「空白处焦点」：没有它，Esc 这类窗口级快捷键
    /// 在没点过任何控件之前收不到（按键只会派给有焦点的那条链）。
    pub(crate) root_focus: FocusHandle,
    pub(crate) settings_focus: FocusHandle,
    /// 日志区的滚动位置（日志多了要能翻）
    pub log_scroll: ScrollHandle,
    /// 上一次渲染时看到的日志版本号：变了就把日志区贴底。
    pub log_revision_seen: Option<u64>,
    /// 诊断用：tick 跑过的次数（判断主循环是否还活着）
    pub tick_count: u64,
    /// 诊断用：主窗口收到的点击次数
    pub click_probe: u32,
    /// 是否已经把主题同步给 gpui-kit 的组件（Switch/TabBar 读的是它）
    pub kit_dark: Option<bool>,
    /// 系统标题栏是否已经切到对应明暗（DWM 要等窗口真正创建后再设）
    pub titlebar_dark: Option<bool>,
    /// 已经设过标题栏配色的窗口（主窗口自绘，设置窗口是系统标题栏）
    pub titled_windows: std::collections::HashSet<WindowId>,
    /// 关窗提示勾了「不再提示」
    pub close_hint_muted: bool,
    /// 用户点了关闭按钮（由窗口回调置位，界面下一帧消费）
    pub close_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// 生成下拉选项文本：`number_labels(1, 10, 1, 0)` → ["1".."10"]，
/// `number_labels(0, 23, 1, 2)` → ["00".."23"]。宽度 0 表示不补零。
/// 把组件库的下拉塞进一个定宽的盒子里。
///
/// 库的 `Select` 内部用了 `size_full()`，外层宽度由父级决定；直接 `.w(px(..))`
/// 会被内部撑开，所以统一用这个盒子里定宽。
pub fn select_box(width: f32, select: impl IntoElement) -> AnyElement {
    div().w(px(width)).flex_none().child(select).into_any_element()
}

fn number_labels(from: u32, to: u32, step: u32, width: usize) -> Vec<SharedString> {
    let mut out = Vec::new();
    let mut value = from;
    while value <= to {
        let text = if width > 0 {
            format!("{value:0width$}")
        } else {
            value.to_string()
        };
        out.push(SharedString::from(text));
        value += step.max(1);
    }
    out
}

impl AppView {
    pub fn new(
        state: AppState,
        runtime: std::sync::Arc<crate::runtime::Runtime>,
        main_window_id: WindowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // 用上次存下来的主题启动（缺字段/旧配置 → 跟随系统）
        let dark = state.theme.is_dark(system_is_dark());
        // 截图/QA：AUTOSIGN_OPEN=qr|time 在建好视图后打开对应弹窗
        let open_for_qa = std::env::var("AUTOSIGN_OPEN").ok();

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

        // QA 回归用：AUTOSIGN_TEST_TRAY=runnow|pause|open 启动 3 秒后往托盘通道塞一条命令，
        // 验证「托盘点击 → channel → 界面立即处理」这条链路（不必真的去点托盘）。
        if let Ok(which) = std::env::var("AUTOSIGN_TEST_TRAY") {
            let command = match which.as_str() {
                "pause" => Some(crate::system::tray::TrayCommand::TogglePause),
                "open" => Some(crate::system::tray::TrayCommand::Open),
                "settings" => Some(crate::system::tray::TrayCommand::Settings),
                _ => Some(crate::system::tray::TrayCommand::RunNow),
            };
            if let Some(command) = command {
                // 延时可调：验证「隐藏 → 再唤起」时需要在触发前先手动藏起来
                let delay = std::env::var("AUTOSIGN_TEST_TRAY_DELAY_MS")
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(3000);
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(delay))
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        let sent = view.runtime.send_tray_command(command);
                        view.state.log(
                            LogKind::Info,
                            format!("QA：模拟托盘命令 {command:?}（发出={sent}）"),
                        );
                        cx.notify();
                    });
                    // 隔 5 秒再发一条，用来判断「消费任务是否还活着」
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(5))
                        .await;
                    // 先把设置窗口关掉（现在是隐藏，不再销毁）
                    let _ = this.update(cx, |view, cx| {
                        if let Some(id) = view.settings_window {
                            if let Some(handle) = cx
                                .windows()
                                .into_iter()
                                .find(|handle| handle.window_id() == id)
                            {
                                let _ = handle.update(cx, |_, window, _| {
                                    crate::system::win::hide_window(window);
                                });
                            }
                        }
                        cx.notify();
                    });
                    let _ = this.update(cx, |view, cx| {
                        let sent = view
                            .runtime
                            .send_tray_command(crate::system::tray::TrayCommand::RunNow);
                        view.state.log(
                            LogKind::Info,
                            format!("QA：第二条命令 RunNow（发出={sent}）"),
                        );
                        cx.notify();
                    });
                })
                .detach();
            }
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
                // 走真实路径：点关闭 → tick 用窗口自己的句柄隐藏
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.close_signal
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    cx.notify();
                });
                eprintln!("QA: close requested");
                // 再从托盘命令唤回，验证「隐藏 → 打开」整条链
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.runtime
                        .send_tray_command(crate::system::tray::TrayCommand::Open);
                    cx.notify();
                });
                eprintln!("QA: tray open sent");
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

        let root_focus = cx.focus_handle();
        let settings_focus = cx.focus_handle();

        // ---- 文本输入：组件库的 Input + InputState ----
        // 编辑、光标、选区、粘贴、中文输入法都由库负责，我们只管「值变了写回草稿」。
        let draft = state.draft.clone();
        let lat_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("39.908720")
                .default_value(draft.lat.clone())
        });
        let lng_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("116.397450")
                .default_value(draft.lng.clone())
        });
        let wx_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("SCT 开头的 SendKey")
                .default_value(draft.wx_key.clone())
        });
        let qq_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Qmsg 酱的 Key")
                .default_value(draft.qq_key.clone())
        });

        cx.subscribe_in(&lat_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let text = this.lat_input.read(cx).value().to_string();
                this.state.edit(|draft| draft.lat = text);
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&lng_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let text = this.lng_input.read(cx).value().to_string();
                this.state.edit(|draft| draft.lng = text);
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&wx_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let text = this.wx_input.read(cx).value().to_string();
                this.state.edit(|draft| draft.wx_key = text);
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&qq_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let text = this.qq_input.read(cx).value().to_string();
                this.state.edit(|draft| draft.qq_key = text);
                cx.notify();
            }
        })
        .detach();

        // ---- 下拉：全部用组件库的 Select（重试次数 / 重试间隔 / 时 / 分）----
        // 库自己管弹出层、限高滚动、键盘上下、点外面收起，所以之前那套
        // 「浮层遮罩 + 手算坐标 + 分列 + occlude」全部删掉了。
        let attempts_index = state.settings.retry_attempts.clamp(1, 10) as usize - 1;
        let delay_index = state.settings.retry_delay_minutes.clamp(1, 60) as usize - 1;
        let retry_attempts_select = cx.new(|cx| {
            SelectState::new(
                number_labels(1, 10, 1, 0),
                Some(IndexPath::new(attempts_index)),
                window,
                cx,
            )
        });
        let retry_delay_select = cx.new(|cx| {
            SelectState::new(
                number_labels(1, 60, 1, 0),
                Some(IndexPath::new(delay_index)),
                window,
                cx,
            )
        });
        cx.subscribe_in(&retry_attempts_select, window, |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(value)) = event {
                if let Ok(attempts) = value.to_string().parse::<u32>() {
                    let _ = this.state.set_retry_attempts(attempts);
                    cx.notify();
                }
            }
        })
        .detach();
        cx.subscribe_in(&retry_delay_select, window, |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(value)) = event {
                if let Ok(delay) = value.to_string().parse::<u32>() {
                    let _ = this.state.set_retry_delay(delay);
                    cx.notify();
                }
            }
        })
        .detach();
        let mut view = Self {
            state,
            runtime,
            dark,
            lat_input,
            lng_input,
            wx_input,
            qq_input,
            inputs_synced: None,
            root_focus,
            settings_focus,
            add_time: String::new(),
            error: None,
            settings_tab: SettingsTab::General,
            qr: None,
            qr_image: None,
            qr_error: None,
            retry_attempts_select,
            retry_delay_select,
            settings_window: None,
            settings_opening: false,
            main_window_id: Some(main_window_id),
            main_hidden: false,
            log_scroll: ScrollHandle::new(),
            log_revision_seen: None,
            tick_count: 0,
            click_probe: 0,
            kit_dark: None,
            titlebar_dark: None,
            titled_windows: Default::default(),
            close_hint_muted: false,
            close_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        // 让主窗口的根节点拿到初始焦点：这样 Esc / Ctrl+Enter 在「还没点过任何控件」时也能用
        window.focus(&view.root_focus, cx);
        // 自绘标题栏 + 系统边框：交给系统处理拖动/双击/snap/拉伸
        crate::system::win::install_frame_subclass(window);

        // 截图/QA：弹窗必须等窗口自己的 Root 装好才能开
        // （open_dialog 依赖窗口级的组件状态，在视图构造期间调用会 panic），
        // 所以这里延后一拍再开。
        match open_for_qa.as_deref() {
            Some("time") => {
                let this = cx.entity();
                cx.spawn_in(window, async move |_, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(400))
                        .await;
                    let _ = this.update_in(cx, |view, window, cx| {
                        let times = view.state.settings.times.clone();
                        crate::ui::time_picker::open(cx.entity().downgrade(), &times, window, cx);
                    });
                })
                .detach();
            }
            Some("qr") => view.start_qr_login(window, cx),
            _ => {}
        }

        // QA：AUTOSIGN_TEST_SCENE=1 复现用户操作序列
        // 开设置 → 隐藏设置 → 隐藏主窗口 → 再开设置（应能弹出）
        if std::env::var("AUTOSIGN_TEST_SCENE").is_ok() {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(3000))
                    .await;
                let _ = this.update(cx, |view, cx| view.open_settings(cx));
                crate::ui_debug("SCENE: 1 settings opened");

                cx.background_executor()
                    .timer(std::time::Duration::from_millis(3000))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    if let Some(id) = view.settings_window {
                        if let Some(handle) =
                            cx.windows().into_iter().find(|h| h.window_id() == id)
                        {
                            let _ = handle.update(cx, |_, window, _| {
                                crate::system::win::hide_window(window);
                            });
                        }
                    }
                    cx.notify();
                });
                crate::ui_debug("SCENE: 2 settings hidden");

                cx.background_executor()
                    .timer(std::time::Duration::from_millis(3000))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.close_signal
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    cx.notify();
                });
                crate::ui_debug("SCENE: 3 main closed");

                cx.background_executor()
                    .timer(std::time::Duration::from_millis(4000))
                    .await;
                // 走真实路径：托盘命令 → 消费任务 → apply_tray_command
                let result = this.update(cx, |view, cx| {
                    let sent = view
                        .runtime
                        .send_tray_command(crate::system::tray::TrayCommand::Settings);
                    // 顺便验证：主窗口隐藏后，这个 entity 还能不能被更新
                    let probe = view.tick_probe();
                    (sent, probe, cx.entity())
                });
                match &result {
                    Ok((sent, probe, _)) => {
                        crate::ui_debug(&format!("SCENE: 4 update ok, sent={sent}, tick={probe}"))
                    }
                    Err(_) => crate::ui_debug("SCENE: 4 update FAILED (entity 不可更新)"),
                }
            })
            .detach();
        }

        // QA：AUTOSIGN_THEME=dark 强制深色，用来截图核对主题
        if std::env::var("AUTOSIGN_THEME").as_deref() == Ok("dark") {
            view.dark = true;
        }
        let _ = view.sync_kit_theme(cx);

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
    /// 托盘事件消费：命令一到就立刻处理并重绘，不等下一次 tick。
    ///
    /// 之前是「托盘写共享标志 → 主线程每秒 tick 读一次」，点了最坏要等 1 秒，
    /// 手感上就像没反应。改成 async_channel 之后是即时的。
    fn spawn_tray_events(&mut self, cx: &mut Context<Self>) {
        let Some(receiver) = self.runtime.take_tray_events() else {
            return;
        };
        // 同样不绑窗口：主窗口隐藏后 update_in 会失败，命令会被静默丢弃
        cx.spawn(async move |this, cx| {
            while let Ok(command) = receiver.recv().await {
                if this
                    .update(cx, |view, cx| view.apply_tray_command(command, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// 执行一条托盘命令。
    fn apply_tray_command(
        &mut self,
        command: crate::system::tray::TrayCommand,
        cx: &mut Context<Self>,
    ) {
        use crate::system::tray::TrayCommand;
        crate::ui_debug(&format!("apply_tray_command: {command:?}"));
        match command {
            TrayCommand::Open => {
                // 走缓存的 HWND：隐藏状态下依然有效
                crate::system::win::show_main_window();
                self.main_hidden = false;
                // 窗口是被 ShowWindow 直接显示的，gpui 在 WM_SHOWWINDOW 里画的那一帧
                // 拿不到新的布局/场景 —— 右上角的「— ✕」要等鼠标移进窗口才补上。
                // 这里主动刷新：先立刻刷一次，再隔一帧补一次（显示动画期间可能被打断）。
                if let Some(main) = self.main_window_id {
                    if let Some(handle) =
                        cx.windows().into_iter().find(|h| h.window_id() == main)
                    {
                        let _ = handle.update(cx, |_, window, _| window.refresh());
                    }
                }
                cx.notify();
                let main = self.main_window_id;
                cx.spawn(async move |_this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(120))
                        .await;
                    let Some(main) = main else { return };
                    let handle = cx.update(|app| {
                        app.windows().into_iter().find(|h| h.window_id() == main)
                    });
                    if let Some(handle) = handle {
                        let _ = handle.update(cx, |_, window, _| window.refresh());
                    }
                })
                .detach();
            }
            TrayCommand::Settings => {
                // 只弹设置页：主窗口该藏着就继续藏着
                self.open_settings(cx);
            }
            TrayCommand::RunNow => {
                self.run_now();
            }
            TrayCommand::TogglePause => {
                let next = !self.state.paused;
                self.runtime.set_paused(next);
                self.state.paused = next;
                self.state.log(
                    LogKind::Info,
                    if next {
                        "托盘：已暂停全部账号".to_string()
                    } else {
                        "托盘：已恢复全部账号".to_string()
                    },
                );
            }
            TrayCommand::ToggleAutostart => {
                let next = !crate::system::autostart::is_enabled();
                crate::system::autostart::set_enabled(next);
                self.state.log(
                    LogKind::Info,
                    if next {
                        "托盘：已开启开机自启".to_string()
                    } else {
                        "托盘：已关闭开机自启".to_string()
                    },
                );
            }
            TrayCommand::Quit => std::process::exit(0),
        }
        cx.notify();
    }

    pub fn start_ticker(&mut self, cx: &mut Context<Self>) {
        self.spawn_tray_events(cx);
        // 用 App 级 spawn，不绑窗口：
        // 绑窗口的 spawn_in 在主窗口隐藏后会 update_in 失败，tick 直接停，
        // 表现就是「最小化到托盘后所有界面逻辑都不走了」。
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1000))
                    .await;
                match this.update(cx, |view, cx| {
                    view.tick(cx);
                    // 窗口藏在托盘里时没人看，不用每秒重绘一帧。
                    // （重绘会带着文本排版缓存、字形图集一起长，后台常驻没必要）
                    if !view.main_hidden {
                        cx.notify();
                    }
                }) {
                    Ok(()) => {}
                    Err(error) => {
                        crate::ui_debug(&format!("ticker: update failed: {error}"));
                        break;
                    }
                }
            }
        })
        .detach();
    }

    /// 同步一次后台状态。
    /// 把明暗状态同步给 gpui-kit 的全局主题。
    ///
    /// Switch / TabBar 这些组件读的是 `cx.theme()`，不是我们的 Palette。
    /// 不同步的话，深色模式下组件仍然是浅色配色（白底选中块、深色文字），
    /// 在深色面板上非常刺眼。
    pub fn sync_kit_theme(&mut self, cx: &mut App) -> bool {
        if self.kit_dark == Some(self.dark) {
            return false;
        }
        self.kit_dark = Some(self.dark);
        let mode = if self.dark {
            gpui_kit::component::ThemeMode::Dark
        } else {
            gpui_kit::component::ThemeMode::Light
        };
        gpui_kit::component::Theme::change(mode, None, cx);
        // 立刻把「所有窗口」的标题栏都刷一遍：等 tick 或等下次渲染都会有肉眼可见的延迟
        self.sync_titlebars(cx);
        true
    }

    /// 给每个窗口单独设一次系统标题栏明暗。
    ///
    /// 设置窗口是后来才建的，它创建时拿到的是系统默认（浅色）。只在「主题变化」
    /// 的那一刻刷一遍会漏掉它 —— 深色外观下设置页顶着一条白标题栏就是这么来的。
    /// 这里按窗口各记一笔，新窗口进来补一次，不用每秒重复调 DWM。
    fn sync_titlebars(&mut self, cx: &mut App) {
        let dark = self.dark;
        if self.titlebar_dark != Some(dark) {
            self.titlebar_dark = Some(dark);
            self.titled_windows.clear();
        }
        let mut done = Vec::new();
        for handle in cx.windows() {
            let id = handle.window_id();
            if self.titled_windows.contains(&id) {
                continue;
            }
            let ok = handle
                .update(cx, |_, window, _| {
                    crate::system::win::set_dark_titlebar(window, dark);
                })
                .is_ok();
            if ok {
                done.push(id);
            }
        }
        for id in done {
            self.titled_windows.insert(id);
        }
    }

    /// 诊断用：看一眼 tick 是否还在跑。
    fn tick_probe(&self) -> u64 {
        self.tick_count
    }

    pub fn tick(&mut self, cx: &mut Context<Self>) {
        self.tick_count = self.tick_count.saturating_add(1);
        self.sync_kit_theme(cx);
        // 标题栏由 DWM 画：新开的窗口（设置页）要在这里补一次
        self.sync_titlebars(cx);
        let (running, paused, next, last_exit) = self.runtime.snapshot();
        self.state.running = running;
        self.state.paused = paused;
        self.state.next_run_at = next;
        self.state.last_exit = last_exit;
        for (kind, line) in self.runtime.drain() {
            self.state.log(kind, line);
        }
        if self.qr.is_some() {
            // 扫码会话进行中：每轮把后台线程的最新状态搬进界面
            let waiting = crate::ui::qr_modal::poll(self);
            cx.notify();
            if !waiting {
                // 会话结束（成功或失败）：把扫码弹窗收起来
                if let Some(id) = self.main_window_id {
                    if let Some(handle) = cx.windows().into_iter().find(|h| h.window_id() == id) {
                        let _ = handle.update(cx, |_, window, cx| window.close_dialog(cx));
                    }
                }
            }
        }
        // 关窗兜底：系统关窗（Alt+F4 / 任务栏关闭）没有 Window 引用，
        // 由 main.rs 的 should_close 回调置标志，这里收尾。
        // 点 ✕ 与 Esc 已经在 request_close 里当场处理，不会走到这里。
        if self
            .close_signal
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            if self.state.settings.minimize_to_tray {
                // 这里没有 window 引用，用缓存的主窗口句柄（隐藏前会先枚举并记住）
                crate::system::win::hide_main_window();
                self.main_hidden = true;
                self.state.log(LogKind::Info, "窗口已隐藏到托盘，签到继续在后台运行");
                // 关闭就是安静地收到托盘：已经有托盘图标和日志，不再弹提示窗
            } else {
                std::process::exit(0);
            }
        }

        // 托盘命令已改成 channel 即时推送（见 spawn_tray_events），这里不再轮询
    }

    fn palette(&self) -> &'static Palette {
        if self.dark {
            &DARK
        } else {
            &LIGHT
        }
    }




    /// 扫码添加：后台线程去拉二维码，成功后自动插入账号。
    ///
    /// 弹窗用组件库 Dialog，内容是一个会自己刷新的视图（要跟着二维码变）。
    fn start_qr_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_qr_session(cx);
        crate::ui::qr_modal::QrModal::open(cx.entity().downgrade(), window, cx);
    }

    /// 只起会话、不动弹窗（「重新生成」用）。
    pub fn start_qr_session(&mut self, cx: &mut Context<Self>) {
        self.qr_error = None;
        self.qr_image = None;
        self.state.log(LogKind::Info, "正在获取登录二维码…");
        self.qr = Some(crate::ui::qr_modal::start(self.state.paths.config_file()));
        cx.notify();
    }

    /// 重新生成二维码：弹窗不关，换一张新码。
    pub fn restart_qr(&mut self, cx: &mut Context<Self>) {
        self.start_qr_session(cx);
    }

    /// 放弃扫码：关掉会话，清掉图与错误。
    pub fn cancel_qr(&mut self, cx: &mut Context<Self>) {
        self.qr = None;
        self.qr_image = None;
        self.qr_error = None;
        self.state.log(LogKind::Info, "已取消扫码");
        cx.notify();
    }

    /// 打开设置窗口（独立窗口：可拖动、可最小化，不再挤在主界面里当覆盖层）。
    ///
    /// 开窗必须放到事件回调之外：`open_window` 会立刻渲染这个 View，
    /// 而此时 entity 正在被 update —— 直接调用会 panic（already being updated）。
    /// 所以统一走「spawn 一次 + AsyncApp::update」，在借用释放后再开窗。
    ///
    /// 这里同时兜住一个老 bug：设置窗口如果被系统回收（或渲染失败没建起来），
    /// 记录里的 `settings_window` 会变成一个不存在的 id，之后每次点设置都走
    /// 「复用已有窗口」这条岔路、永远找不到 → 表现就是「设置再也打不开」。
    /// 现在查不到就把记录清掉并重建。
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        // 开窗要走一次异步任务，连点两下会开出两个窗口（第二个会把记录覆盖掉，
        // 留下一个没人管的孤儿设置窗口），这里用标记挡住。
        if self.settings_opening {
            return;
        }
        self.settings_opening = true;

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(260.0), px(180.0)),
                size: size(px(SETTINGS_W), px(SETTINGS_H)),
            })),
            // 与主窗口同一套：不要系统标题栏，标题栏由我们自己画
            window_decorations: Some(gpui_kit::WindowDecorations::Client),
            titlebar: Some(TitlebarOptions {
                title: Some("设置".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_min_size: Some(size(px(720.0), px(460.0))),
            ..Default::default()
        };

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1))
                .await;

            // 1) 已经开着就把它叫到前面（16ms 之后才碰 cx.windows()：
            //    托盘命令是在 entity 的 update 闭包里进来的，同步调用会 panic）
            let existing = this
                .update(cx, |view, _| view.settings_window)
                .ok()
                .flatten();
            if let Some(id) = existing {
                let handle = cx.update(|app| {
                    app.windows()
                        .into_iter()
                        .find(|handle| handle.window_id() == id)
                });
                if let Some(handle) = handle {
                    let _ = handle.update(cx, |_, window, _| {
                        crate::system::win::show_window(window);
                        window.activate_window();
                    });
                    crate::ui_debug("open_settings: reused window shown");
                    let _ = this.update(cx, |view, _| view.settings_opening = false);
                    return;
                }
                // 窗口没了：清掉陈旧记录，往下重建
                crate::ui_debug("open_settings: stale window id, recreating");
                let _ = this.update(cx, |view, cx| {
                    view.settings_window = None;
                    cx.notify();
                });
            }

            // 2) 新建
            let Some(entity) = this.upgrade() else {
                return;
            };
            crate::ui_debug("open_settings: creating window");
            let opened = cx.update(|app| app.open_window(options, move |_, _| entity.clone()));
            match &opened {
                Ok(handle) => {
                    let id = handle.window_id();
                    // 让设置窗口的根节点拿初始焦点（Esc 关闭要靠它）
                    let focus = this
                        .update(cx, |view, _| view.settings_focus.clone())
                        .ok();
                    if let Some(focus) = focus {
                        let _ = handle.update(cx, |_, window, cx| {
                            window.focus(&focus, cx);
                            // 设置窗口也是自绘标题栏，同样交给系统
                            crate::system::win::install_frame_subclass(window);
                        });
                    }
                    crate::ui_debug("open_settings: created ok");
                    // 设置窗口现在也是客户端装饰（标题栏自绘），不用再刷 DWM 深色标题栏
                    let _ = this.update(cx, |view, cx| {
                        view.settings_window = Some(id);
                        view.settings_opening = false;
                        cx.notify();
                    });
                }
                Err(error) => {
                    crate::ui_debug(&format!("open_settings: create failed: {error}"));
                    // 建不出来也不能把记录留在半路，否则下次还是走复用岔路
                    let _ = this.update(cx, |view, cx| {
                        view.settings_window = None;
                        view.settings_opening = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// 关闭设置窗口（设置页里的「完成」与标题栏 ✕ 都走这里）。
    pub fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = window.window_handle().window_id();
        // 判据用「这不是主窗口」，而不是「等于记录的 settings_window」：
        // 记录万一过期，「完成」按钮就会变成点了没反应。这里以事实为准。
        if self.main_window_id == Some(id) {
            self.state.log(
                LogKind::Warn,
                "关闭设置窗口被忽略：当前是主窗口".to_string(),
            );
            cx.notify();
            return;
        }
        // 只隐藏，不销毁：销毁共用同一 entity 的窗口会把整个 app 的消息循环卡死。
        crate::system::win::hide_window(window);
        // 两个窗口都藏起来 = 完全在后台，这时候再让内存
        if self.main_hidden {
            crate::system::win::enter_background();
        }
        // 隐藏后把前台交还主窗口：否则 gpui 可能仍把激活留在被隐藏的设置窗口上，
        // 主窗口看起来"点不动"（事件进不来）。
        // 但主窗口若本来就在托盘里藏着，就别把它叫出来。
        if !self.main_hidden {
            crate::system::win::show_main_window();
        }
        cx.notify();
    }

    /// 切换主题：只换配色，不改窗口显隐。
    ///
    /// 之前是从设置页里直接改 `dark` 再 `sync_kit_theme`，而 `Theme::change`
    /// 会刷新所有窗口 —— 一旦某个窗口被顺带显示/顶到别的窗口后面，
    /// 用户看到的就是「设置页没了、主窗口冒出来」。
    /// 时间选择器点「添加」：写进设置里的时间列表。
    ///
    /// 返回 `Err` 时弹窗留在原地显示原因（重复、格式非法等），不静默失败。
    pub fn add_time_from_picker(&mut self, value: &str, cx: &mut Context<Self>) -> Result<(), String> {
        match self.state.add_time(value) {
            Ok(()) => {
                self.error = None;
                cx.notify();
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.0.clone());
                cx.notify();
                Err(error.0)
            }
        }
    }

    /// 把草稿里的值推回四个输入框。
    ///
    /// 只在「草稿被外部重灌」时调用（切账号 / 保存 / 取消编辑）——
    /// 用户打字时不回推，否则会打断库输入框里正在移动的光标。
    pub fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.state.draft.clone();
        self.lat_input.update(cx, |state, cx| {
            state.set_value(draft.lat.clone(), window, cx);
        });
        self.lng_input.update(cx, |state, cx| {
            state.set_value(draft.lng.clone(), window, cx);
        });
        self.wx_input.update(cx, |state, cx| {
            state.set_value(draft.wx_key.clone(), window, cx);
        });
        self.qq_input.update(cx, |state, cx| {
            state.set_value(draft.qq_key.clone(), window, cx);
        });
    }

    pub fn switch_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        if self.state.theme == mode && self.dark == mode.is_dark(system_is_dark()) {
            return;
        }
        self.dark = mode.is_dark(system_is_dark());
        self.sync_kit_theme(cx);

        // 落盘：下次启动还是这个主题。写失败只记一行日志，不打断界面切换。
        if let Err(error) = self.state.save_theme(mode) {
            self.state
                .log(LogKind::Warn, format!("主题已切换，但{}", error.0));
        }

        self.restore_window_visibility(cx);
        cx.notify();
    }

    /// 主题刷新后把窗口显隐恢复原样（该藏的继续藏，设置窗口保持可见）。
    fn restore_window_visibility(&mut self, cx: &mut App) {
        // 主窗口该藏就继续藏着（切主题不负责把它叫出来）
        if self.main_hidden {
            crate::system::win::hide_main_window();
        }
        if let Some(id) = self.settings_window {
            if let Some(handle) = cx.windows().into_iter().find(|h| h.window_id() == id) {
                // 这里**不能**让它抢前台，否则会把用户正在看的设置页顶到后面；
                // 也不能碰主窗口，否则主窗口会被激活盖住设置页。
                let _ = handle.update(cx, |_, window, _| {
                    crate::system::win::keep_visible_without_activate(window);
                });
            }
        }
    }

    /// Esc：按「从内到外」的顺序关一层。
    ///
    /// 下拉浮层 → 弹窗覆盖层 → 都没有就把窗口收到托盘（和点 ✕ 一致）。
    /// 参考 ElegantClipboard 的 dismiss_or_hide：先收临时层，最后才动窗口。
    fn dismiss_or_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 有弹窗就先关弹窗（Esc 由库负责，这里兜住「弹窗打开时按 Esc」的路径）
        if window.has_active_dialog(cx) {
            window.close_dialog(cx);
            return;
        }
        self.request_close(window, cx);
    }

    /// 关窗：按设置决定「收进托盘」还是「直接退出」，并且**当场**执行。
    ///
    /// 原来是把 `close_signal` 置位、等下一次 tick 才隐藏 —— tick 是 1 秒一次，
    /// 于是点 ✕ 之后窗口最多要等一秒才消失，看起来就是「点了没反应」。
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.settings.minimize_to_tray {
            // 记住 HWND：窗口隐藏后枚举不到，托盘「打开主窗口」全靠这个句柄
            crate::system::win::hide_main_window_of(window);
            self.main_hidden = true;
            self.state
                .log(LogKind::Info, "窗口已隐藏到托盘，签到继续在后台运行");
            cx.notify();
        } else {
            std::process::exit(0);
        }
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
        self.state.log_revision = self.state.log_revision.wrapping_add(1);
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

    /// 当前渠道的密钥（未打码）。
    pub fn channel_key(&self, field: Field) -> String {
        match field {
            Field::Qq => self.state.draft.qq_key.clone(),
            _ => self.state.draft.wx_key.clone(),
        }
    }

    /// 「编辑」：弹窗里改该渠道的密钥（草稿先按磁盘内容重灌一次）。
    fn open_channel_editor(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        crate::ui::channel_modal::open(self, field, window, cx);
    }

    /// 保存弹窗里改过的密钥：只动这一个字段，不碰坐标与重试设置。
    pub fn save_channel_key(&mut self, field: Field, cx: &mut Context<Self>) {
        let index = self.state.selected;
        let key = self.channel_key(field).trim().to_string();
        crate::ui_debug(&format!("save_channel_key len={}", key.len()));
        if let Some(student) = self.state.config.students.get_mut(index) {
            match field {
                Field::Qq => student.qmsg_key = key.clone(),
                _ => student.wx_key = key.clone(),
            }
        }
        let name = channel_name(field);
        match self.state.config.save(self.state.paths.config_file()) {
            Ok(()) => {
                self.state.log(
                    LogKind::Ok,
                    if key.is_empty() {
                        format!("已清空{name}密钥，该渠道不会推送")
                    } else {
                        format!("已保存{name}密钥（{}）", mask_key(&key))
                    },
                );
                self.state.reload_draft();
            }
            Err(error) => self
                .state
                .log(LogKind::Bad, format!("保存{name}密钥失败：{error}")),
        }
        cx.notify();
    }

    /// 放弃编辑：把草稿按磁盘内容重灌。
    pub fn cancel_channel_edit(&mut self, cx: &mut Context<Self>) {
        self.state.reload_draft();
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
        let was_selected = index;

        // 「失败 / 需登录 / 单独暂停」都是按账号下标记的，删之前先整体挪位：
        // 不挪的话，删掉中间账号会让后面的人继承前一个人的状态。
        let paused = self.runtime.paused_accounts();
        self.state.on_account_removed(index);
        for old in &paused {
            self.runtime.set_account_paused(*old, false);
        }
        for old in paused {
            match old.cmp(&index) {
                std::cmp::Ordering::Less => self.runtime.set_account_paused(old, true),
                std::cmp::Ordering::Greater => self.runtime.set_account_paused(old - 1, true),
                std::cmp::Ordering::Equal => {} // 被删掉的那个，暂停状态一起丢掉
            }
        }

        let removed = self.state.config.students.remove(index);
        let _ = self.state.config.save(self.state.paths.config_file());
        if self.state.students().is_empty() {
            self.state.selected = 0;
            self.state.reload_draft();
        } else {
            // 删的是别人：当前选中的人不变（下标要 -1）；
            // 删的正是当前这个人：顺位选下一个，末尾则退一个。
            let next = if was_selected > index {
                was_selected - 1
            } else {
                index.min(self.state.students().len() - 1)
            };
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
                    // 拖动 / 双击最大化 / 拖到边缘吸附全部交给系统：
                    // win::install_frame_subclass 在 WM_NCHITTEST 里把这条带子
                    // 报成 HTCAPTION，系统自己跑模态拖动循环。
                    // （以前自己跟光标 SetWindowPos，鼠标在窗口外松开时收不到
                    //  mouse_up，window_drag 就永远留着 —— 表现是「一抓住就松不开」。）
                    .child(img("logo-64.png").w(px(20.0)).h(px(20.0)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(p.text_2))
                            .child("班级魔方自动签到"),
                    )
                    .child(pill(text, fg, bg, border, true)),
            )
            // 按钮区不落在拖拽区里，否则点不到（系统会把按下当成拖动）
            .child(window_button(WindowButton::Min, p, cx))
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
                                    .path(IconName::User.path())
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
                                    .on_click(click(cx, |this, _, window, cx| {
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
        // 开机自启 / 关窗行为只放在设置页，侧栏保持干净
        div()
            .border_t_1()
            .border_color(rgb(p.line))
            .px(px(12.0))
            .py(px(9.0))
            .flex()
            .flex_col()
            .gap(px(7.0))
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
                        .on_click(click(cx, |this, _, _, cx| {
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
                        .on_click(click(cx, |this, _, _, cx| {
                            let dir = this.state.data_dir().display().to_string();
                            let _ = open_url(&dir);
                            cx.notify();
                        })),
                )
                .child(
                    button("重新扫码", BtnKind::Primary, BtnSize::Small, false, p)
                        .id("relogin")
                        .cursor_pointer()
                        .on_click(click(cx, |this, _, window, cx| {
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
                        .on_click(click(cx, |this, _, _, cx| {
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
                        .on_click(click(cx, |this, _, _, cx| {
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
                    .path(IconName::TriangleAlert.path())
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

        let lat = self.text_field(Field::Lat, readonly, cx);
        let lng = self.text_field(Field::Lng, readonly, cx);
        // 通知渠道：开关只决定「发不发」，输入框不再直接铺在这里。
        // 关掉时整块收起来，打开时在右侧给「编辑 / 查看」两个入口，各自弹窗操作。
        let (wx_on, qq_on) = self
            .state
            .selected_student()
            .map(|student| (student.wx_enabled(), student.qq_enabled()))
            .unwrap_or((false, false));

        let pin = svg()
            .path(IconName::Map.path())
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
                                    .on_click(click(cx, |this, _, _, cx| {
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
                            .child(notify_block(
                                "微信通知 · Server 酱",
                                Field::Wx,
                                wx_on,
                                p,
                                cx,
                            ))
                            .child(notify_block(
                                "QQ 通知 · Qmsg 酱",
                                Field::Qq,
                                qq_on,
                                p,
                                cx,
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
                    // 不再让用户手打 21:31:00，直接弹时间列表点选
                    crate::ui::time_picker::open(cx.entity().downgrade(), &this.state.settings.times, window, cx);
                }))
                .into_any_element()
        });

        // 重试次数与间隔都改成下拉点选，不再手输数字
        // 宽度靠外层定死：库的 Select 内部是 size_full()，直接 .w() 不一定生效
        let attempts = select_box(
            64.0,
            Select::new(&self.retry_attempts_select)
                .small()
                .disabled(readonly),
        );
        let delay = select_box(
            64.0,
            Select::new(&self.retry_delay_select)
                .small()
                .disabled(readonly),
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
                    .on_click(click(cx, |this, _, _, cx| {
                        this.delete_selected();
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                button("保存", BtnKind::Primary, BtnSize::Small, !self.state.dirty, p)
                    .id("save")
                    .cursor_pointer()
                    .on_click(click(cx, |this, _, _, cx| this.save(cx)))
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
                                            .on_click(click(cx, |this, _, window, cx| {
                                                this.start_qr_login(window, cx);
                                            })),
                                    )
                                    .child(
                                        button("导入配置文件", BtnKind::Default, BtnSize::Normal, false, p)
                                            .id("import-config")
                                            .cursor_pointer()
                                            .on_click(click(cx, |this, _, _, cx| {
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

    /// 一个文本输入框：直接用组件库的 Input。
    ///
    /// 光标、选区、左右键、Home/End、退格、粘贴、中文输入法全部由库处理；
    /// 我们只在 `InputEvent::Change` 里把文本写回草稿。
    pub(crate) fn text_field(
        &self,
        field: Field,
        readonly: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = match field {
            Field::Lat => self.lat_input.clone(),
            Field::Lng => self.lng_input.clone(),
            Field::Qq => self.qq_input.clone(),
            _ => self.wx_input.clone(),
        };
        Input::new(&state).disabled(readonly).into_any_element()
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
    Close,
}

/// 标题栏的最小化 / 最大化 / 关闭。
///
/// 之前用 `window_control_area`，那是把命中判定交给系统的 WM_NCHITTEST ——
/// 鼠标在标题栏移动时系统会不停地问「这点是什么区域」，叠上 hover 重绘就明显卡顿
/// （设置窗口用的是系统标题栏，所以没事）。改成普通点击 + 显式 API。
pub(crate) fn window_button(kind: WindowButton, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let (glyph, danger) = match kind {
        WindowButton::Min => ("─", false),
        WindowButton::Close => ("✕", true),
    };
    let hover_fg = if danger { 0xffffff } else { p.text };

    div()
        .id(match kind {
            WindowButton::Min => "win-min",
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
            // 关闭 = 交给「最小化到托盘 / 直接退出」那条路，但**立刻**执行：
            // 原来是置一个标志、等下一次 tick（最长 1 秒）才隐藏，手感就是「点了没反应」。
            // 设置窗口是同一套标题栏，关闭要收自己（隐藏），不能把主窗口收进托盘。
            WindowButton::Close => {
                if this.main_window_id == Some(window.window_handle().window_id()) {
                    this.request_close(window, cx);
                } else {
                    this.close_settings(window, cx);
                }
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

/// 通知渠道名（日志与弹窗标题共用）。
pub fn channel_name(field: Field) -> &'static str {
    match field {
        Field::Qq => "QQ 通知",
        _ => "微信通知",
    }
}

/// 渠道的说明文案（Server 酱 / Qmsg 酱）。
pub fn channel_hint(field: Field) -> &'static str {
    match field {
        Field::Qq => "在 Qmsg 酱后台复制 Key，形如 xxxx",
        _ => "在 Server 酱后台复制 SendKey，形如 SCTxxxx",
    }
}

/// 渠道密钥在 data.json 里的字段名。
pub fn channel_field_label(field: Field) -> &'static str {
    match field {
        Field::Qq => "QmsgKEY",
        _ => "WXKey",
    }
}

/// 通知渠道一行：开关 + 名称 + 右侧「编辑 / 查看」。
///
/// 关掉时不再显示输入框（也不显示假输入框），只留一句「已关闭」；
/// 打开时keys 的读写都走弹窗：编辑 = 改 Key，查看 = 打码看一眼。
/// 这样主界面上不会有半个屏幕的密钥输入框，也不容易误改。
fn notify_block(
    title: &'static str,
    field: Field,
    enabled: bool,
    p: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let app = cx.entity();
    // 只让 Switch 自己处理点击。整行再挂一个 on_click 的话，
    // 一次点击会走两遍切换（Switch 一次 + 行一次），净效果等于没点。
    let toggle = Switch::new(SharedString::from(format!("notify-switch-{title}")))
        .checked(enabled)
        .color(Hsla::from(rgb(p.accent)))
        .on_change(move |next: &bool, _window, cx: &mut App| {
            let next = *next;
            app.update(cx, |this, cx| {
                this.set_channel(field, next);
                let name = channel_name(field);
                this.state.log(
                    LogKind::Info,
                    if next {
                        format!("已开启{name}，点「编辑」填入密钥后即可推送")
                    } else {
                        format!("已关闭{name}，密钥仍保留在 data.json 里")
                    },
                );
                // 关掉渠道时把草稿重灌一次：正在编辑的密钥不会漏进保存流程
                if !next {
                    this.state.reload_draft();
                }
                cx.notify();
            });
        });

    let mut actions = div().flex().items_center().gap(px(4.0));
    if enabled {
        actions = actions
            .child(
                button("编辑", BtnKind::Default, BtnSize::Xs, false, p)
                    .id(SharedString::from(format!(
                        "channel-edit-{}",
                        channel_field_label(field)
                    )))
                    .cursor_pointer()
                    .on_click(click(cx, move |this, _, window, cx| {
                        this.open_channel_editor(field, window, cx);
                    })),
            )
            // 「查看」去掉：密钥在「编辑」弹窗里就是明文，一眼能看全，
            // 多一个只读弹窗反而多一层点击。
            ;
    } else {
        actions = actions.child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(p.text_3))
                .child("已关闭，不发送"),
        );
    }

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .h(px(30.0))
                .w_full()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(toggle)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.0))
                        .text_color(rgb(p.text_2))
                        .child(title),
                )
                .child(actions),
        )
        .into_any_element()
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
pub(crate) fn set_clipboard_text(text: &str) -> std::io::Result<()> {
    copy_to_clipboard(text)
}

/// 对外的一层：弹窗里的「复制」也走这里，保证编码一致。
pub fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    set_clipboard_impl(text)
}

fn set_clipboard_impl(text: &str) -> std::io::Result<()> {
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

        let window_id = window.window_handle().window_id();

        // 同一个 entity 在两个窗口里渲染：只有主窗口画主界面，其余一律是设置页。
        // 之前是「window_id 等于 settings_window 才画设置」，一旦 settings_window
        // 记录丢了，设置窗口就会把主界面画出来 —— 用户看到的是「设置页没了、
        // 主窗口冒出来」。改成以主窗口为基准，两边都不会认错。
        if self.main_window_id != Some(window_id) {
            return crate::ui::settings::render(self, p, cx);
        }

        // 草稿被外部重灌（切账号 / 保存 / 取消编辑）时，把值推回输入框。
        // 用版本号而不是每次渲染都推：后者会在用户打字时把光标打回末尾。
        if self.inputs_synced != Some(self.state.draft_revision) {
            self.inputs_synced = Some(self.state.draft_revision);
            self.sync_inputs(window, cx);
        }

        // 日志贴底：只在**渲染这一帧**把请求交给 ScrollHandle，
        // 它才会在紧接着的 prepaint 里用「含新行」的内容高度算偏移。
        // 在 tick / 点击回调里提前调的话，会被「还没包含新行」的那一帧先消费掉，
        // 结果永远差一行到不了底。版本号比长度可靠：600 行封顶后长度不再变化。
        if self.log_revision_seen != Some(self.state.log_revision) {
            self.log_revision_seen = Some(self.state.log_revision);
            self.log_scroll.scroll_to_bottom();
        }

        let mut root = div()
            .v_flex()
            .size_full()
            .track_focus(&self.root_focus)
            .bg(rgb(p.desk))
            .text_size(px(13.0))
            .text_color(rgb(p.text))
            // 状态栏写着 Ctrl+Enter 立即签到，这里把它真正接上
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if event.keystroke.modifiers.control && key == "enter" {
                    this.run_now();
                    cx.notify();
                    return;
                }
                if key == "escape" {
                    this.dismiss_or_hide(window, cx);
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

        root = root.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, _| {
                // 只作为诊断计数保留：输入框的失焦由组件库的 Input 自己处理
                let _ = event;
                this.click_probe = this.click_probe.saturating_add(1);
            }),
        );

        root.into_any_element()
    }
}

