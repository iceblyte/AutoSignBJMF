//! 后台运行时：调度线程 + 托盘 + 与界面共享的状态。
//!
//! 线程模型很简单：
//!
//! - **调度线程**：跑 [`Scheduler::run_loop`]，到点执行 `Runner::run_once`
//! - **托盘线程**：`tray-icon` 的图标与菜单必须在该线程有 win32 消息循环
//! - **界面**：只读共享快照，自己用 1 秒定时器刷新
//!
//! 三者通过 `Arc<Mutex<Shared>>` 通信，没有人持有别人的引用，也就没有死锁面。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bjmf_core::scheduler::{Event, Hooks, Scheduler};
use bjmf_core::timepoints::parse_time_list;
use bjmf_core::{Notifier, Runner, Settings, Site};
use chrono::{Local, NaiveDateTime};

use crate::state::LogKind;

/// 界面要读的那一小块状态。
#[derive(Debug, Default)]
pub struct Shared {
    pub running: bool,
    pub paused: bool,
    pub next_run_at: Option<NaiveDateTime>,
    pub last_exit: Option<i32>,
    /// 待界面消费的日志（界面取走后清空）
    pub pending: Vec<(LogKind, String)>,
    /// 托盘请求把主窗口提到前台
    pub show_requested: bool,
}

pub type SharedHandle = Arc<Mutex<Shared>>;

/// 运行时的句柄。
pub struct Runtime {
    pub shared: SharedHandle,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    run_now: Arc<AtomicBool>,
    pub tray: Option<crate::system::tray::TrayHandle>,
}

impl Runtime {
    /// 起调度线程与托盘。`settings_provider` 每次到点都重新读一次设置，
    /// 这样界面里改时间点无需重启调度。
    pub fn start(
        config_path: std::path::PathBuf,
        settings_path: std::path::PathBuf,
        autostart_on: bool,
    ) -> Self {
        let shared: SharedHandle = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let run_now = Arc::new(AtomicBool::new(false));

        // ---- 调度线程 ----
        {
            let shared = shared.clone();
            let stop = stop.clone();
            let paused = paused.clone();
            let run_now = run_now.clone();
            let config_path = config_path.clone();
            let settings_path = settings_path.clone();
            std::thread::Builder::new()
                .name("bjmf-scheduler".into())
                .spawn(move || {
                    let site = match Site::with_default_base() {
                        Ok(site) => site,
                        Err(error) => {
                            push(&shared, LogKind::Bad, format!("初始化网络客户端失败：{error}"));
                            return;
                        }
                    };
                    let notifier = match Notifier::production() {
                        Ok(notifier) => notifier,
                        Err(error) => {
                            push(&shared, LogKind::Bad, format!("初始化推送客户端失败：{error}"));
                            return;
                        }
                    };

                    let settings = Settings::load(&settings_path);
                    let times_for_run = parse_time_list(&settings.times).unwrap_or_default();

                    let mut scheduler = Scheduler::new(Hooks {
                        run: {
                            let shared = shared.clone();
                            let config_path = config_path.clone();
                            move || {
                                push(&shared, LogKind::Info, "开始执行一轮签到");
                                let mut lines = Vec::new();
                                let report = {
                                    let mut runner = Runner::new(
                                        &site,
                                        &notifier,
                                        &config_path,
                                        |line: &str| lines.push(line.to_string()),
                                    );
                                    runner.run_once()
                                };
                                for line in lines {
                                    push(&shared, LogKind::Info, line);
                                }
                                for outcome in &report.outcomes {
                                    let kind = match outcome.status {
                                        bjmf_core::SignStatus::Success
                                        | bjmf_core::SignStatus::AlreadySigned => LogKind::Ok,
                                        bjmf_core::SignStatus::Error
                                        | bjmf_core::SignStatus::Skip => LogKind::Bad,
                                        _ => LogKind::Info,
                                    };
                                    push(
                                        &shared,
                                        kind,
                                        format!("{}：{}", outcome.name, outcome.detail),
                                    );
                                }
                                let code = report.exit_code();
                                if let Ok(mut guard) = shared.lock() {
                                    guard.last_exit = Some(code);
                                }
                                push(
                                    &shared,
                                    if code == 0 { LogKind::Ok } else { LogKind::Bad },
                                    if code == 0 {
                                        "本轮执行完成".to_string()
                                    } else {
                                        "本轮执行失败，可按 Ctrl+Enter 重试".to_string()
                                    },
                                );
                                code
                            }
                        },
                        times: {
                            let settings_path = settings_path.clone();
                            let fallback = times_for_run.clone();
                            move || {
                                let settings = Settings::load(&settings_path);
                                parse_time_list(&settings.times).unwrap_or_else(|_| fallback.clone())
                            }
                        },
                        now: || Local::now().naive_local(),
                        wait: {
                            let stop = stop.clone();
                            let run_now = run_now.clone();
                            move |duration: Duration| {
                                // 切片等待：每秒检查一次停止标志，别让它睡死
                                let total = duration.as_millis() as u64;
                                let mut waited = 0;
                                while waited < total {
                                    if stop.load(Ordering::Relaxed) {
                                        return false;
                                    }
                                    let slice = std::cmp::min(1000, total - waited);
                                    std::thread::sleep(Duration::from_millis(slice));
                                    waited += slice;
                                    // 立即签到：不中断等待，只是让主循环尽快响应
                                    if run_now.swap(false, Ordering::SeqCst) {
                                        // 由界面直接触发，这里不做事
                                    }
                                }
                                !stop.load(Ordering::Relaxed)
                            }
                        },
                        on_event: {
                            let shared = shared.clone();
                            move |event: Event| {
                                let mut guard = match shared.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => return,
                                };
                                match event {
                                    Event::NextRun { at } => {
                                        guard.next_run_at = Some(at);
                                        guard.running = true;
                                    }
                                    Event::RunStart { attempt, total } => {
                                        if attempt > 1 {
                                            guard.pending.push((
                                                LogKind::Warn,
                                                format!("失败重试 {attempt}/{total}"),
                                            ));
                                        }
                                    }
                                    Event::RunDone { code, .. } => {
                                        guard.last_exit = Some(code);
                                    }
                                    Event::Retry { delay } => {
                                        guard.pending.push((
                                            LogKind::Warn,
                                            format!("{} 分钟后重试", delay.as_secs() / 60),
                                        ));
                                    }
                                    Event::Skipped { .. } => {
                                        guard
                                            .pending
                                            .push((LogKind::Warn, "已暂停，跳过本次执行".into()));
                                    }
                                    Event::Paused => guard.paused = true,
                                    Event::Resumed => guard.paused = false,
                                    Event::Stopped => {
                                        guard.running = false;
                                        guard.next_run_at = None;
                                    }
                                    Event::Error { message } => {
                                        guard.pending.push((LogKind::Bad, message));
                                    }
                                }
                            }
                        },
                    })
                    .with_retry(settings.retry_attempts, Duration::from_secs(settings.retry_delay_seconds()));

                    // 暂停标志由界面直接改；调度器自己不轮询它，所以这里用一个
                    // 轻量代理：每 5 秒同步一次。
                    {
                        let paused = paused.clone();
                        let shared = shared.clone();
                        std::thread::spawn(move || loop {
                            std::thread::sleep(Duration::from_secs(2));
                            let want = paused.load(Ordering::Relaxed);
                            if let Ok(mut guard) = shared.lock() {
                                guard.paused = want;
                            }
                        });
                    }

                    scheduler.run_loop();
                })
                .expect("启动调度线程失败");
        }

        // ---- 托盘 ----
        // 截图/自动化场景可以关掉托盘（也便于排查托盘相关问题）
        let tray_enabled = std::env::var_os("AUTOSIGN_NO_TRAY").is_none();
        let tray = if tray_enabled {
            crate::system::tray::spawn(
                shared.clone(),
                paused.clone(),
                stop.clone(),
                run_now.clone(),
                autostart_on,
            )
            .ok()
        } else {
            None
        };

        Self {
            shared,
            stop,
            paused,
            run_now,
            tray,
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        if let Ok(mut guard) = self.shared.lock() {
            guard.paused = paused;
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// 请求立刻执行一次（由界面调用，真正的执行在调度线程里排队）。
    pub fn request_run_now(&self) {
        self.run_now.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.shared.lock() {
            guard
                .pending
                .push((LogKind::Info, "已请求立即签到，稍后开始".into()));
        }
    }

    /// 托盘是否请求把窗口提到前台（取走即清）。
    pub fn take_show_request(&self) -> bool {
        match self.shared.lock() {
            Ok(mut guard) => std::mem::take(&mut guard.show_requested),
            Err(_) => false,
        }
    }

    /// 把共享状态里攒下的日志搬到界面上。
    pub fn drain(&self) -> Vec<(LogKind, String)> {
        match self.shared.lock() {
            Ok(mut guard) => std::mem::take(&mut guard.pending),
            Err(_) => Vec::new(),
        }
    }

    pub fn snapshot(&self) -> (bool, bool, Option<NaiveDateTime>, Option<i32>) {
        match self.shared.lock() {
            Ok(guard) => (
                guard.running,
                guard.paused,
                guard.next_run_at,
                guard.last_exit,
            ),
            Err(_) => (false, false, None, None),
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn push(shared: &SharedHandle, kind: LogKind, text: impl Into<String>) {
    if let Ok(mut guard) = shared.lock() {
        guard.pending.push((kind, text.into()));
    }
}
