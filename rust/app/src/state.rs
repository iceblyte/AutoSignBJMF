//! 界面状态：把"界面上能看到的每个字段"收在一处，逻辑全部可测。
//!
//! 渲染层只做两件事：读这里的数据、把点击转成这里的方法。
//! 这样"编辑校验""状态判定""文案格式"都能脱离窗口单测，
//! 不用起 GUI 也能覆盖。
//!
//! 对应关系：
//! - `Draft` ↔ 主界面「签到地点与通知」「每天执行时间」两张卡的可编辑字段
//! - `AccountStatus` ↔ 原型里的四态（运行中 / 需重新登录 / 签到失败 / 已暂停）
//! - `title_status` ↔ 标题栏右侧那个 pill

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use bjmf_core::timepoints::{humanize_delta, parse_time_list};
use bjmf_core::{Config, Paths, Settings, Student};
use chrono::{Local, NaiveDateTime};

use crate::theme::ThemeMode;

/// 日志行的语气，决定颜色（原型 `.log .ok` / `.wn` / `.bd`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Ok,
    Warn,
    Bad,
}

impl LogKind {
    /// 落盘时用的级别标记。
    pub fn label(self) -> &'static str {
        match self {
            LogKind::Info => "INFO",
            LogKind::Ok => "OK",
            LogKind::Warn => "WARN",
            LogKind::Bad => "BAD",
        }
    }

    /// 从落盘文件里读回。
    pub fn from_label(text: &str) -> LogKind {
        match text {
            "OK" => LogKind::Ok,
            "WARN" => LogKind::Warn,
            "BAD" => LogKind::Bad,
            _ => LogKind::Info,
        }
    }
}

/// 解析一行落盘日志：`HH:MM:SS [KIND] text`。格式不对就丢掉。
fn parse_log_line(line: &str) -> Option<LogLine> {
    let (time, rest) = line.split_once(' ')?;
    let rest = rest.strip_prefix('[')?;
    let (kind, text) = rest.split_once("] ")?;
    Some(LogLine {
        time: time.to_string(),
        kind: LogKind::from_label(kind),
        text: text.to_string(),
    })
}

/// 一行日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// `HH:MM:SS`
    pub time: String,
    pub text: String,
    pub kind: LogKind,
}

/// 账号在界面上显示的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Running,
    NeedsLogin,
    Failed,
    Paused,
    Idle,
}

impl AccountStatus {
    pub fn label(self) -> &'static str {
        match self {
            AccountStatus::Running => "运行中",
            AccountStatus::NeedsLogin => "需重新登录",
            AccountStatus::Failed => "签到失败",
            AccountStatus::Paused => "已暂停",
            AccountStatus::Idle => "未运行",
        }
    }

    /// 侧栏副标题的第二段（原型里 `· 下次 21:31` / `· 已暂停`）。
    pub fn is_problem(self) -> bool {
        matches!(self, AccountStatus::NeedsLogin | AccountStatus::Failed)
    }
}

/// 标题栏 pill 的语气。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillTone {
    Ok,
    Warn,
    Plain,
}

/// 可编辑草稿：界面上的输入框都绑到这里，保存时才写盘。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    pub lat: String,
    pub lng: String,
    pub acc: String,
    pub wx_key: String,
    pub qq_key: String,
    pub retry_attempts: String,
    pub retry_delay: String,
}

impl Draft {
    pub fn from_student(student: &Student, settings: &Settings) -> Self {
        Self {
            lat: student.lat.clone(),
            lng: student.lng.clone(),
            acc: if student.acc.trim().is_empty() {
                settings.acc.clone()
            } else {
                student.acc.clone()
            },
            wx_key: student.wx_key.clone(),
            qq_key: student.qmsg_key.clone(),
            retry_attempts: settings.retry_attempts.to_string(),
            retry_delay: settings.retry_delay_minutes.to_string(),
        }
    }
}

/// 校验后的坐标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coords {
    pub lat: f64,
    pub lng: f64,
    pub acc: u32,
}

/// 保存失败的说明，直接拿去显示在输入框下面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError(pub String);

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 解析并校验坐标与精度。
pub fn validate_coords(lat: &str, lng: &str, acc: &str) -> Result<Coords, FieldError> {
    let lat_value: f64 = lat
        .trim()
        .parse()
        .map_err(|_| FieldError("纬度必须是数字，例如 39.908720".into()))?;
    let lng_value: f64 = lng
        .trim()
        .parse()
        .map_err(|_| FieldError("经度必须是数字，例如 116.397450".into()))?;
    if !(-90.0..=90.0).contains(&lat_value) {
        return Err(FieldError("纬度需要在 -90 到 90 之间".into()));
    }
    if !(-180.0..=180.0).contains(&lng_value) {
        return Err(FieldError("经度需要在 -180 到 180 之间".into()));
    }
    let acc_value: u32 = acc
        .trim()
        .parse()
        .map_err(|_| FieldError("精度必须是整数米数".into()))?;
    if acc_value == 0 || acc_value > 1000 {
        return Err(FieldError("精度需要在 1–1000 米之间".into()));
    }
    Ok(Coords {
        lat: lat_value,
        lng: lng_value,
        acc: acc_value,
    })
}

/// 解析重试次数与间隔，区间与设计稿一致（1–10 次 / 1–60 分钟）。
pub fn validate_retry(attempts: &str, delay: &str) -> Result<(u32, u32), FieldError> {
    let attempts_value: u32 = attempts
        .trim()
        .parse()
        .map_err(|_| FieldError("重试次数必须是整数".into()))?;
    let delay_value: u32 = delay
        .trim()
        .parse()
        .map_err(|_| FieldError("重试间隔必须是整数分钟".into()))?;
    if !(1..=10).contains(&attempts_value) {
        return Err(FieldError("重试次数需要在 1–10 之间".into()));
    }
    if !(1..=60).contains(&delay_value) {
        return Err(FieldError("重试间隔需要在 1–60 分钟之间".into()));
    }
    Ok((attempts_value, delay_value))
}

/// 把密钥掩码成 `SCT••••••4f2a`。
///
/// 短于 8 位的一律全遮蔽，避免把整串漏出去。
pub fn mask_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "•".repeat(chars.len().max(4));
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}••••••{tail}")
}

/// 应用状态。
pub struct AppState {
    pub paths: Paths,
    pub config: Config,
    pub settings: Settings,
    pub selected: usize,
    /// 后台调度是否在跑。
    pub running: bool,
    /// 用户点了暂停（到点不执行）。
    pub paused: bool,
    pub next_run_at: Option<NaiveDateTime>,
    pub last_exit: Option<i32>,
    pub logs: Vec<LogLine>,
    pub draft: Draft,
    pub dirty: bool,
    pub theme: ThemeMode,
    /// 最近一轮里失败的账号下标。
    failed: BTreeSet<usize>,
    /// 需要重新登录的账号下标（Cookie 拉不到资料时标记）。
    needs_login: BTreeSet<usize>,
}

impl AppState {
    /// 从磁盘读配置与设置。读坏了也照常起来（各自回退默认值）。
    pub fn load(paths: Paths) -> Self {
        let _ = paths.ensure();
        let config = Config::load(paths.config_file()).unwrap_or_default();
        let settings = Settings::load(paths.settings_file());
        let selected = if config.students.is_empty() { 0 } else { 0 };
        let draft = config
            .students
            .first()
            .map(|student| Draft::from_student(student, &settings))
            .unwrap_or_else(|| Draft {
                retry_attempts: settings.retry_attempts.to_string(),
                retry_delay: settings.retry_delay_minutes.to_string(),
                acc: settings.acc.clone(),
                ..Draft::default()
            });
        // 启动时接上今天已经写下的日志（最近 200 行）
        let logs = Self::load_recent_logs(&paths, 200);
        Self {
            paths,
            config,
            settings,
            selected,
            running: false,
            paused: false,
            next_run_at: None,
            last_exit: None,
            logs,
            draft,
            dirty: false,
            theme: ThemeMode::System,
            failed: BTreeSet::new(),
            needs_login: BTreeSet::new(),
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.paths.root().to_path_buf()
    }

    pub fn students(&self) -> &[Student] {
        &self.config.students
    }

    pub fn selected_student(&self) -> Option<&Student> {
        self.config.students.get(self.selected)
    }

    pub fn has_accounts(&self) -> bool {
        !self.config.students.is_empty()
    }

    /// 切到另一个账号：草稿跟着换，未保存的改动丢弃（与原型一致）。
    pub fn select(&mut self, index: usize) {
        if index >= self.config.students.len() {
            return;
        }
        self.selected = index;
        self.reload_draft();
    }

    /// 重新从账号数据灌草稿。
    pub fn reload_draft(&mut self) {
        self.draft = match self.selected_student() {
            Some(student) => Draft::from_student(student, &self.settings),
            None => Draft {
                retry_attempts: self.settings.retry_attempts.to_string(),
                retry_delay: self.settings.retry_delay_minutes.to_string(),
                acc: self.settings.acc.clone(),
                ..Draft::default()
            },
        };
        self.dirty = false;
    }

    /// 改草稿里的任意字段。
    pub fn edit<F: FnOnce(&mut Draft)>(&mut self, change: F) {
        change(&mut self.draft);
        self.dirty = true;
    }

    /// 保存草稿：先校验，再写 `data.json` 与 `settings.json`。
    ///
    /// 返回 `Err` 时什么都不写，界面保留用户输入。
    pub fn save_draft(&mut self) -> Result<(), FieldError> {
        let coords = validate_coords(&self.draft.lat, &self.draft.lng, &self.draft.acc)?;
        let (attempts, delay) = validate_retry(&self.draft.retry_attempts, &self.draft.retry_delay)?;

        if let Some(student) = self.config.students.get_mut(self.selected) {
            student.lat = format!("{:.6}", coords.lat).trim_end_matches('0').trim_end_matches('.').to_string();
            student.lng = format!("{:.6}", coords.lng).trim_end_matches('0').trim_end_matches('.').to_string();
            student.acc = coords.acc.to_string();
            student.wx_key = self.draft.wx_key.trim().to_string();
            student.qmsg_key = self.draft.qq_key.trim().to_string();
        }
        self.settings.retry_attempts = attempts;
        self.settings.retry_delay_minutes = delay;

        self.config
            .save(self.paths.config_file())
            .map_err(|error| FieldError(format!("保存配置失败：{error}")))?;
        self.settings
            .save(self.paths.settings_file())
            .map_err(|error| FieldError(format!("保存设置失败：{error}")))?;

        self.dirty = false;
        self.reload_draft();
        Ok(())
    }

    /// 加一个执行时间点。重复或格式错误返回错误文案。
    pub fn add_time(&mut self, raw: &str) -> Result<(), FieldError> {
        let parsed = parse_time_list([raw.trim()]).map_err(|error| FieldError(error.to_string()))?;
        let value = parsed[0].to_string();
        if self.settings.times.iter().any(|existing| existing == &value) {
            return Err(FieldError(format!("{value} 已经在列表里了")));
        }
        self.settings.times.push(value);
        self.settings.times.sort();
        self.settings
            .save(self.paths.settings_file())
            .map_err(|error| FieldError(format!("保存设置失败：{error}")))
    }

    /// 改失败重试次数（1–10，超界收敛到边界）。
    pub fn set_retry_attempts(&mut self, value: u32) -> Result<(), FieldError> {
        let clamped = value.clamp(1, 10);
        self.settings.retry_attempts = clamped;
        self.settings
            .save(self.paths.settings_file())
            .map_err(|error| FieldError(format!("保存设置失败：{error}")))
    }

    /// 改失败重试间隔（1–60 分钟）。
    pub fn set_retry_delay(&mut self, value: u32) -> Result<(), FieldError> {
        let clamped = value.clamp(1, 60);
        self.settings.retry_delay_minutes = clamped;
        self.settings
            .save(self.paths.settings_file())
            .map_err(|error| FieldError(format!("保存设置失败：{error}")))
    }

    /// 删一个执行时间点。至少留一个（否则调度器无事可做）。
    pub fn remove_time(&mut self, value: &str) -> Result<(), FieldError> {
        if self.settings.times.len() <= 1 {
            return Err(FieldError("至少要保留一个执行时间".into()));
        }
        self.settings.times.retain(|existing| existing != value);
        self.settings
            .save(self.paths.settings_file())
            .map_err(|error| FieldError(format!("保存设置失败：{error}")))
    }

    /// 账号状态（原型四态 + 未运行）。
    pub fn status_of(&self, index: usize) -> AccountStatus {
        let Some(student) = self.config.students.get(index) else {
            return AccountStatus::Idle;
        };
        if student.cookie.trim().is_empty() || self.needs_login.contains(&index) {
            return AccountStatus::NeedsLogin;
        }
        if self.failed.contains(&index) {
            return AccountStatus::Failed;
        }
        if self.paused {
            return AccountStatus::Paused;
        }
        if !self.running || self.settings.times.is_empty() {
            return AccountStatus::Idle;
        }
        AccountStatus::Running
    }

    /// 侧栏一行的副标题：`2023 级计算机 1 班 · 下次 21:31`。
    pub fn subtitle_of(&self, index: usize) -> String {
        let Some(student) = self.config.students.get(index) else {
            return String::new();
        };
        let class = if student.class.trim().is_empty() {
            "未填写班级".to_string()
        } else {
            student.class.clone()
        };
        let status = self.status_of(index);
        match status {
            AccountStatus::NeedsLogin => format!("{class} · 需重新登录"),
            AccountStatus::Failed => format!("{class} · 本次签到失败"),
            AccountStatus::Paused => format!("{class} · 已暂停"),
            _ => match self.next_run_at {
                Some(next) => format!("{class} · 下次 {}", next.format("%H:%M")),
                None => format!("{class} · 尚未开始调度"),
            },
        }
    }

    /// 主区大标题下的一行说明。
    pub fn headline_of(&self, index: usize) -> String {
        let Some(student) = self.config.students.get(index) else {
            return String::new();
        };
        let class = if student.class.trim().is_empty() {
            "未填写班级".to_string()
        } else {
            student.class.clone()
        };
        match self.status_of(index) {
            AccountStatus::NeedsLogin => format!("{class} · 微信登录凭证已过期"),
            AccountStatus::Failed => format!("{class} · 上次执行失败，可查看日志"),
            _ => match self.next_run_at {
                Some(next) => {
                    let delta = (next - Local::now().naive_local()).num_seconds().max(0);
                    format!("{class} · 下次 {}（{}后）", next.format("%H:%M"), humanize_delta(delta))
                }
                None => format!("{class} · 尚未开始调度"),
            },
        }
    }

    /// 标题栏 pill：`(语气, 文案)`。
    pub fn title_status(&self) -> (PillTone, String) {
        let problems = self
            .config
            .students
            .iter()
            .enumerate()
            .filter(|(index, student)| {
                student.cookie.trim().is_empty()
                    || self.needs_login.contains(index)
                    || self.failed.contains(index)
            })
            .count();
        if problems > 0 {
            return (PillTone::Warn, format!("{problems} 个账号需处理"));
        }
        if !self.running {
            return (PillTone::Plain, "未运行".to_string());
        }
        if self.paused {
            return (PillTone::Plain, "已暂停".to_string());
        }
        (PillTone::Ok, "后台运行中".to_string())
    }

    /// 状态栏左侧的一句话。
    pub fn status_line(&self) -> String {
        if !self.has_accounts() {
            return "未运行".to_string();
        }
        if !self.running {
            return "未运行".to_string();
        }
        if self.paused {
            return "已暂停 · 到点不执行".to_string();
        }
        match self.next_run_at {
            Some(next) => {
                let delta = (next - Local::now().naive_local()).num_seconds().max(0);
                format!("运行中 · 下次 {} · 还有 {}", next.format("%H:%M"), humanize_delta(delta))
            }
            None => "运行中 · 正在计算下次执行时间…".to_string(),
        }
    }

    /// 状态栏右侧的保存提示。
    pub fn save_hint(&self) -> &'static str {
        if self.dirty {
            "有未保存的修改"
        } else {
            "无未保存的修改"
        }
    }

    /// 追加一行日志，最多留 600 行（原型日志面板是定高的）。
    pub fn log(&mut self, kind: LogKind, text: impl Into<String>) {
        let text = text.into();
        let time = Local::now().format("%H:%M:%S").to_string();
        self.append_to_file(&time, kind, &text);
        self.logs.push(LogLine {
            time,
            text,
            kind,
        });
        let overflow = self.logs.len().saturating_sub(600);
        if overflow > 0 {
            self.logs.drain(0..overflow);
        }
    }

    /// 今天的日志文件：`<数据目录>/logs/app-YYYY-MM-DD.log`。
    pub fn log_file(&self) -> std::path::PathBuf {
        self.paths
            .log_dir()
            .join(format!("app-{}.log", Local::now().format("%Y-%m-%d")))
    }

    /// 追加一行到日志文件。写失败不影响界面（日志是辅助信息）。
    fn append_to_file(&self, time: &str, kind: LogKind, text: &str) {
        use std::io::Write;
        let path = self.log_file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "{time} [{}] {text}", kind.label());
        }
    }

    /// 读回今天的日志（启动时把上次运行的最后几行接上）。
    fn load_recent_logs(paths: &Paths, limit: usize) -> Vec<LogLine> {
        let path = paths
            .log_dir()
            .join(format!("app-{}.log", Local::now().format("%Y-%m-%d")));
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        let mut lines: Vec<LogLine> = text
            .lines()
            .filter_map(parse_log_line)
            .collect();
        if lines.len() > limit {
            lines.drain(0..lines.len() - limit);
        }
        lines
    }

    /// 把执行结果写进状态：失败记到 `failed`，凭证失效记到 `needs_login`。
    pub fn apply_outcome(&mut self, index: usize, status: bjmf_core::SignStatus) {
        match status {
            bjmf_core::SignStatus::Skip => {
                self.needs_login.insert(index);
            }
            bjmf_core::SignStatus::Error => {
                self.failed.insert(index);
            }
            bjmf_core::SignStatus::Success | bjmf_core::SignStatus::AlreadySigned => {
                self.failed.remove(&index);
                self.needs_login.remove(&index);
            }
            _ => {
                self.failed.remove(&index);
            }
        }
    }

    /// 一轮结束后的整体收尾。
    pub fn apply_report(&mut self, report: &bjmf_core::Report) {
        self.running = true;
        self.last_exit = Some(report.exit_code());
        for (index, outcome) in report.outcomes.iter().enumerate() {
            self.apply_outcome(index, outcome.status);
        }
        for outcome in &report.outcomes {
            let kind = match outcome.status {
                bjmf_core::SignStatus::Success | bjmf_core::SignStatus::AlreadySigned => LogKind::Ok,
                bjmf_core::SignStatus::Error | bjmf_core::SignStatus::Skip => LogKind::Bad,
                _ => LogKind::Info,
            };
            self.log(kind, format!("{}：{}", outcome.name, outcome.detail));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_paths(name: &str) -> Paths {
        let dir = std::env::temp_dir().join("bjmf-app-state").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Paths::resolve(&dir.join("exe"), &dir.join("home"))
    }

    fn state_with_students(name: &str, json: &str) -> AppState {
        let paths = temp_paths(name);
        paths.ensure().unwrap();
        fs::write(paths.config_file(), json).unwrap();
        AppState::load(paths)
    }

    const TWO: &str = r#"{"students":[
        {"name":"张明远","class":"2000001","lat":"39.908720","lng":"116.397450","acc":"30","cookie":"ck","WXKey":"SCT12345678"},
        {"name":"林可欣","class":"2000002","lat":"","lng":"","acc":"30","cookie":""}
    ]}"#;

    #[test]
    fn 加载配置后草稿跟着第一个账号() {
        let state = state_with_students("load", TWO);
        assert_eq!(state.students().len(), 2);
        assert_eq!(state.draft.lat, "39.908720");
        assert_eq!(state.draft.wx_key, "SCT12345678");
        assert!(!state.dirty);
    }

    #[test]
    fn 切换账号会重灌草稿() {
        let mut state = state_with_students("switch", TWO);
        state.edit(|draft| draft.lat = "1.0".into());
        assert!(state.dirty);

        state.select(1);
        assert_eq!(state.selected, 1);
        assert_eq!(state.draft.lat, "");
        assert!(!state.dirty);
    }

    #[test]
    fn 越界下标不生效() {
        let mut state = state_with_students("bounds", TWO);
        state.select(9);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn 保存草稿会写盘并清掉脏标记() {
        let mut state = state_with_students("save", TWO);
        state.edit(|draft| {
            draft.lat = "39.9".into();
            draft.lng = "116.4".into();
            draft.acc = "25".into();
            draft.wx_key = "  SCT9  ".into();
            draft.retry_attempts = "5".into();
            draft.retry_delay = "10".into();
        });
        state.save_draft().unwrap();

        let saved = Config::load(state.paths.config_file()).unwrap();
        assert_eq!(saved.students[0].lat, "39.9");
        assert_eq!(saved.students[0].acc, "25");
        assert_eq!(saved.students[0].wx_key, "SCT9");
        assert_eq!(state.settings.retry_attempts, 5);
        assert_eq!(state.settings.retry_delay_minutes, 10);
        assert!(!state.dirty);
    }

    #[test]
    fn 非法坐标不写盘() {
        let mut state = state_with_students("bad-coords", TWO);
        state.edit(|draft| {
            draft.lat = "abc".into();
            draft.lng = "116.4".into();
        });
        let error = state.save_draft().unwrap_err();
        assert!(error.0.contains("纬度"));

        // 边界值
        assert!(validate_coords("91", "0", "30").is_err());
        assert!(validate_coords("0", "181", "30").is_err());
        assert!(validate_coords("0", "0", "0").is_err());
        assert!(validate_coords("39.9", "116.4", "30").is_ok());
    }

    #[test]
    fn 重试区间与设计稿一致() {
        assert!(validate_retry("0", "5").is_err());
        assert!(validate_retry("11", "5").is_err());
        assert!(validate_retry("3", "0").is_err());
        assert!(validate_retry("3", "61").is_err());
        assert_eq!(validate_retry("1", "1").unwrap(), (1, 1));
        assert_eq!(validate_retry("10", "60").unwrap(), (10, 60));
    }

    #[test]
    fn 加时间点会排序且拒绝重复() {
        let mut state = state_with_students("times", TWO);
        state.settings.times = vec!["21:31:00".into()];
        state.add_time("7:30").unwrap();
        assert_eq!(state.settings.times, vec!["07:30:00", "21:31:00"]);
        assert!(state.add_time("07:30").is_err());
        assert!(state.add_time("25:00").is_err());
    }

    #[test]
    fn 至少保留一个时间点() {
        let mut state = state_with_students("remove-time", TWO);
        state.settings.times = vec!["21:31:00".into()];
        assert!(state.remove_time("21:31:00").is_err());
        state.settings.times = vec!["07:30:00".into(), "21:31:00".into()];
        state.remove_time("07:30:00").unwrap();
        assert_eq!(state.settings.times, vec!["21:31:00"]);
    }

    #[test]
    fn 状态判定四态() {
        let mut state = state_with_students("status", TWO);
        // 未运行
        assert_eq!(state.status_of(0), AccountStatus::Idle);
        // 运行中
        state.running = true;
        assert_eq!(state.status_of(0), AccountStatus::Running);
        // 暂停
        state.paused = true;
        assert_eq!(state.status_of(0), AccountStatus::Paused);
        // 没 cookie → 需重新登录（优先于暂停）
        assert_eq!(state.status_of(1), AccountStatus::NeedsLogin);
        // 失败
        state.paused = false;
        state.apply_outcome(0, bjmf_core::SignStatus::Error);
        assert_eq!(state.status_of(0), AccountStatus::Failed);
        // 恢复成功
        state.apply_outcome(0, bjmf_core::SignStatus::Success);
        assert_eq!(state.status_of(0), AccountStatus::Running);
    }

    #[test]
    fn 标题栏_pill_随状态变化() {
        let mut state = state_with_students("pill", TWO);
        // 第二个账号没有 cookie
        assert_eq!(state.title_status(), (PillTone::Warn, "1 个账号需处理".to_string()));

        state.config.students[1].cookie = "ck2".into();
        assert_eq!(state.title_status(), (PillTone::Plain, "未运行".to_string()));

        state.running = true;
        assert_eq!(state.title_status(), (PillTone::Ok, "后台运行中".to_string()));

        state.paused = true;
        assert_eq!(state.title_status(), (PillTone::Plain, "已暂停".to_string()));
    }

    #[test]
    fn 侧栏副标题按状态给不同文案() {
        let mut state = state_with_students("subtitle", TWO);
        state.running = true;
        state.next_run_at = Some(Local::now().naive_local() + chrono::Duration::minutes(30));
        let text = state.subtitle_of(0);
        assert!(text.contains("2000001"));
        assert!(text.contains("下次"));

        assert!(state.subtitle_of(1).contains("需重新登录"));

        state.paused = true;
        assert!(state.subtitle_of(0).contains("已暂停"));
    }

    #[test]
    fn 状态栏文案() {
        let mut state = state_with_students("statusline", TWO);
        assert_eq!(state.status_line(), "未运行");
        state.running = true;
        state.next_run_at = None;
        assert!(state.status_line().contains("正在计算"));

        state.next_run_at = Some(Local::now().naive_local() + chrono::Duration::hours(4));
        assert!(state.status_line().contains("还有"));

        state.paused = true;
        assert!(state.status_line().contains("已暂停"));
    }

    #[test]
    fn 保存提示随脏标记变化() {
        let mut state = state_with_students("hint", TWO);
        assert_eq!(state.save_hint(), "无未保存的修改");
        state.edit(|draft| draft.lat = "1".into());
        assert_eq!(state.save_hint(), "有未保存的修改");
    }

    #[test]
    fn 日志最多保留六百行() {
        let mut state = state_with_students("logs", TWO);
        for index in 0..650 {
            state.log(LogKind::Info, format!("第 {index} 行"));
        }
        assert_eq!(state.logs.len(), 600);
        assert!(state.logs.last().unwrap().text.contains("649"));
    }

    #[test]
    fn 密钥掩码只露出首三尾四() {
        assert_eq!(mask_key("SCT1234567890abcd"), "SCT••••••abcd");
        assert_eq!(mask_key("short"), "•••••");
        assert_eq!(mask_key(""), "");
    }

    #[test]
    fn 一轮结束后失败与需登录被记账() {
        let mut state = state_with_students("report", TWO);
        let report = bjmf_core::Report {
            outcomes: vec![
                bjmf_core::SignOutcome::new("张明远", bjmf_core::SignStatus::Success, ""),
                bjmf_core::SignOutcome::new("林可欣", bjmf_core::SignStatus::Skip, ""),
            ],
            config_changed: false,
            notified: false,
        };
        state.apply_report(&report);
        assert!(state.running);
        // 一个成功一个跳过 → 不算"全部硬失败"，退出码 0（与 Python 口径一致）
        assert_eq!(state.last_exit, Some(0));
        assert_eq!(state.status_of(1), AccountStatus::NeedsLogin);
        assert_eq!(state.logs.len(), 2);

        // 全部硬失败才是 1
        let all_failed = bjmf_core::Report {
            outcomes: vec![
                bjmf_core::SignOutcome::new("张明远", bjmf_core::SignStatus::Error, ""),
                bjmf_core::SignOutcome::new("林可欣", bjmf_core::SignStatus::Skip, ""),
            ],
            config_changed: false,
            notified: false,
        };
        state.apply_report(&all_failed);
        assert_eq!(state.last_exit, Some(1));
        assert_eq!(state.status_of(0), AccountStatus::Failed);
    }

    #[test]
    fn 空配置时没有账号也不崩() {
        let state = state_with_students("empty", r#"{"students":[]}"#);
        assert!(!state.has_accounts());
        assert!(state.selected_student().is_none());
        assert_eq!(state.status_of(0), AccountStatus::Idle);
        assert_eq!(state.headline_of(0), "");
    }
}
