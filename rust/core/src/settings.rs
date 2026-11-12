//! 本机设置（`settings.json`）。
//!
//! 对应 Python 的 `desktop/settings.py`：字段名、默认值、收敛规则保持一致，
//! 这样两版可以共用同一个数据目录。差异只有一处，写在 [`normalize`] 的注释里。
//!
//! 原则：**读坏了也不能让程序起不来**。文件缺失、JSON 坏掉、字段类型不对，
//! 一律回退到默认值，而不是返回错误。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::timepoints::parse_time_list;

/// 默认执行时间，与 Python 版一致（21:31 是这个项目的"历史时刻"）。
pub const DEFAULT_TIME: &str = "21:31:00";
pub const DEFAULT_RETRY_ATTEMPTS: u32 = 3;
pub const DEFAULT_RETRY_DELAY_MINUTES: u32 = 5;
pub const MAX_RETRY_ATTEMPTS: u32 = 10;
/// 设计稿写死「1–10 次 · 1–60 分钟」，界面上的可编辑区间就是它。
///
/// Python 版允许 0–120（`retry_delay_minutes`），这里收窄到 1–60：
/// 0 分钟等于立即重试、120 分钟会让一次签到拖到下一个时间点，两者都是误设。
/// 代价是：如果用户在 Python 版存过 120，Rust 版读进来会收敛成 60。
pub const MAX_RETRY_DELAY_MINUTES: u32 = 60;
pub const DEFAULT_ACC: &str = "30";

/// 本机设置。字段与 `settings.json` 同名同序。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 每天的执行时间点，统一为 `HH:MM:SS`，升序去重。
    pub times: Vec<String>,
    /// 单次到点最多执行几次（含首次）。
    pub retry_attempts: u32,
    /// 两次重试之间的间隔（分钟）。
    pub retry_delay_minutes: u32,
    /// 登录 Windows 后自动启动。
    pub autostart: bool,
    /// 关闭窗口时最小化到托盘（false = 直接退出）。
    pub minimize_to_tray: bool,
    /// 签到上报的定位精度（米）。
    pub acc: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            times: vec![DEFAULT_TIME.to_string()],
            retry_attempts: DEFAULT_RETRY_ATTEMPTS,
            retry_delay_minutes: DEFAULT_RETRY_DELAY_MINUTES,
            autostart: false,
            minimize_to_tray: true,
            acc: DEFAULT_ACC.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct SettingsError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "保存设置 {} 失败：{}", self.path.display(), self.source)
    }
}

impl std::error::Error for SettingsError {}

impl Settings {
    /// 把任意来源的原始值收敛成合法设置。
    ///
    /// - `times`：解析失败或为空 → 回退默认值
    /// - `retry_attempts`：夹到 1–10，非数字 → 3
    /// - `retry_delay_minutes`：夹到 1–60，非数字 → 5
    /// - `acc`：空 → "30"，否则原样（去首尾空格）
    pub fn normalize(raw: &serde_json::Value) -> Self {
        let default = Settings::default();
        let Some(obj) = raw.as_object() else {
            return default;
        };

        let times = match obj.get("times") {
            Some(value) => match json_to_time_list(value) {
                Some(list) if !list.is_empty() => list,
                _ => default.times.clone(),
            },
            None => default.times.clone(),
        };

        let retry_attempts = clamp_int(
            obj.get("retry_attempts"),
            1,
            MAX_RETRY_ATTEMPTS,
            DEFAULT_RETRY_ATTEMPTS,
        );
        let retry_delay_minutes = clamp_int(
            obj.get("retry_delay_minutes"),
            1,
            MAX_RETRY_DELAY_MINUTES,
            DEFAULT_RETRY_DELAY_MINUTES,
        );

        let acc = match obj.get("acc") {
            Some(serde_json::Value::String(text)) if !text.trim().is_empty() => {
                text.trim().to_string()
            }
            Some(serde_json::Value::Number(number)) => number.to_string(),
            _ => DEFAULT_ACC.to_string(),
        };

        Self {
            times,
            retry_attempts,
            retry_delay_minutes,
            autostart: obj
                .get("autostart")
                .map(truthy)
                .unwrap_or(default.autostart),
            minimize_to_tray: obj
                .get("minimize_to_tray")
                .map(truthy)
                .unwrap_or(default.minimize_to_tray),
            acc,
        }
    }

    /// 读取设置；文件不存在或内容损坏都返回默认值。
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let Ok(text) = fs::read_to_string(path) else {
            return Settings::default();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Settings::default();
        };
        Settings::normalize(&value)
    }

    /// 归一化后写盘（先写临时文件再改名，避免写一半）。
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SettingsError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| SettingsError {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let normalized = Settings::normalize(
            &serde_json::to_value(self).expect("设置序列化不应失败"),
        );
        let text = serde_json::to_string_pretty(&normalized).expect("设置序列化不应失败");
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|source| SettingsError {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, path).map_err(|source| SettingsError {
            path: path.to_path_buf(),
            source,
        })
    }

    /// 重试间隔换算成秒，给调度器用。
    pub fn retry_delay_seconds(&self) -> u64 {
        u64::from(self.retry_delay_minutes) * 60
    }
}

/// `times` 既可能是数组，也可能是 `"21:31,7:30"` 这种逗号串（老配置文件）。
fn json_to_time_list(value: &serde_json::Value) -> Option<Vec<String>> {
    let raw: Vec<String> = match value {
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| match item {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .collect(),
        serde_json::Value::String(text) => text.split(',').map(|s| s.to_string()).collect(),
        _ => return None,
    };
    parse_time_list(&raw)
        .ok()
        .map(|times| times.iter().map(|clock| clock.to_string()).collect())
}

fn clamp_int(value: Option<&serde_json::Value>, min: u32, max: u32, fallback: u32) -> u32 {
    let Some(value) = value else {
        return fallback;
    };
    let number = match value {
        serde_json::Value::Number(number) => number.as_i64(),
        serde_json::Value::String(text) => text.trim().parse::<i64>().ok(),
        serde_json::Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    };
    match number {
        Some(number) => number.clamp(i64::from(min), i64::from(max)) as u32,
        None => fallback,
    }
}

/// Python 的 `bool(x)`：0、空串、空数组为假，其余为真。
fn truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::Null => false,
        serde_json::Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(false),
        serde_json::Value::String(text) => !text.is_empty(),
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(map) => !map.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("bjmf-core-settings");
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn 文件缺失时用默认值() {
        let loaded = Settings::load(temp("missing.json"));
        assert_eq!(loaded.times, vec!["21:31:00".to_string()]);
        assert_eq!(loaded.retry_attempts, 3);
        assert_eq!(loaded.retry_delay_minutes, 5);
        assert!(loaded.minimize_to_tray);
        assert!(!loaded.autostart);
    }

    #[test]
    fn 文件损坏时用默认值() {
        let path = temp("broken.json");
        fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load(&path).times, vec!["21:31:00".to_string()]);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn 存取往返会归一化时间() {
        let path = temp("roundtrip.json");
        let settings = Settings {
            times: vec!["21:31".to_string(), "7:05".to_string()],
            ..Settings::default()
        };
        settings.save(&path).unwrap();

        let loaded = Settings::load(&path);
        assert_eq!(loaded.times, vec!["07:05:00".to_string(), "21:31:00".to_string()]);

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("07:05:00"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn 非法时间回退默认值() {
        assert_eq!(
            Settings::normalize(&json!({"times": ["25:00"]})).times,
            vec!["21:31:00".to_string()]
        );
        assert_eq!(
            Settings::normalize(&json!({"times": []})).times,
            vec!["21:31:00".to_string()]
        );
        assert_eq!(
            Settings::normalize(&json!({"times": "abc"})).times,
            vec!["21:31:00".to_string()]
        );
    }

    #[test]
    fn 时间可以是逗号串() {
        let settings = Settings::normalize(&json!({"times": "21:31, 7:30"}));
        assert_eq!(settings.times, vec!["07:30:00".to_string(), "21:31:00".to_string()]);
    }

    #[test]
    fn 重试次数被夹到区间内() {
        assert_eq!(Settings::normalize(&json!({"retry_attempts": 0})).retry_attempts, 1);
        assert_eq!(
            Settings::normalize(&json!({"retry_attempts": 999})).retry_attempts,
            MAX_RETRY_ATTEMPTS
        );
        assert_eq!(
            Settings::normalize(&json!({"retry_attempts": "x"})).retry_attempts,
            DEFAULT_RETRY_ATTEMPTS
        );
    }

    #[test]
    fn 重试间隔被夹到区间内() {
        assert_eq!(
            Settings::normalize(&json!({"retry_delay_minutes": -5})).retry_delay_minutes,
            1
        );
        assert_eq!(
            Settings::normalize(&json!({"retry_delay_minutes": 999})).retry_delay_minutes,
            MAX_RETRY_DELAY_MINUTES
        );
        assert_eq!(
            Settings::normalize(&json!({"retry_delay_minutes": 0})).retry_delay_minutes,
            1
        );
        assert_eq!(
            Settings::normalize(&json!({"retry_delay_minutes": "7"})).retry_delay_minutes,
            7
        );
    }

    #[test]
    fn 精度为空时回退30() {
        assert_eq!(Settings::normalize(&json!({"acc": ""})).acc, "30");
        assert_eq!(Settings::normalize(&json!({"acc": " 50 "})).acc, "50");
        assert_eq!(Settings::normalize(&json!({"acc": 25})).acc, "25");
    }

    #[test]
    fn 非对象输入返回默认值() {
        assert_eq!(Settings::normalize(&json!("oops")).times, vec!["21:31:00".to_string()]);
        assert_eq!(Settings::normalize(&json!(null)).times, vec!["21:31:00".to_string()]);
    }

    #[test]
    fn 布尔开关按_python_的真值语义() {
        let normalized = Settings::normalize(&json!({"autostart": 1, "minimize_to_tray": 0}));
        assert!(normalized.autostart);
        assert!(!normalized.minimize_to_tray);

        let normalized = Settings::normalize(&json!({"autostart": "yes"}));
        assert!(normalized.autostart);
    }

    #[test]
    fn 重试间隔换算成秒() {
        let settings = Settings {
            retry_delay_minutes: 7,
            ..Settings::default()
        };
        assert_eq!(settings.retry_delay_seconds(), 420);
    }
}
