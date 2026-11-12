//! 时间点解析与下一次触发计算。
//!
//! 与 `utils/timepoints.py` 逐条对齐：同样的输入、同样的报错条件、同样的
//! `HH:MM:SS` 输出与排序去重规则。CLI 与守护进程共用这一份，避免两处漂移。

use std::fmt;

use chrono::{Duration, NaiveDateTime, NaiveTime, Timelike};

/// 一天内的一个时刻（秒级）。`Ord` 按 时→分→秒 比较，可直接排序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Clock {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl fmt::Display for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

impl Clock {
    pub const fn new(hour: u8, minute: u8, second: u8) -> Self {
        Self {
            hour,
            minute,
            second,
        }
    }

    /// 解析 `H:MM` 或 `HH:MM:SS`（小时允许一位）。
    pub fn parse(raw: &str) -> Result<Self, TimeError> {
        let chunk = raw.trim();
        let parts: Vec<&str> = chunk.split(':').collect();
        if parts.len() != 2 && parts.len() != 3 {
            return Err(TimeError::BadFormat(chunk.to_string()));
        }
        let hour: i32 = parts[0]
            .parse()
            .map_err(|_| TimeError::BadFormat(chunk.to_string()))?;
        let minute: i32 = parts[1]
            .parse()
            .map_err(|_| TimeError::BadFormat(chunk.to_string()))?;
        let second: i32 = match parts.get(2) {
            Some(s) => s
                .parse()
                .map_err(|_| TimeError::BadFormat(chunk.to_string()))?,
            None => 0,
        };
        if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..60).contains(&second) {
            return Err(TimeError::OutOfRange(chunk.to_string()));
        }
        Ok(Clock::new(hour as u8, minute as u8, second as u8))
    }

    /// 严格晚于 `now` 的最近一次该时刻。
    pub fn next_after(&self, now: NaiveDateTime) -> NaiveDateTime {
        let time = NaiveTime::from_hms_opt(self.hour as u32, self.minute as u32, self.second as u32)
            .expect("Clock 已保证取值范围");
        let mut candidate = NaiveDateTime::new(now.date(), time);
        if candidate <= now {
            candidate += Duration::days(1);
        }
        candidate
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeError {
    BadFormat(String),
    OutOfRange(String),
    Empty,
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::BadFormat(s) => {
                write!(f, "时间格式错误: {s}，应形如 HH:MM 或 HH:MM:SS")
            }
            TimeError::OutOfRange(s) => write!(f, "时间超出范围: {s}"),
            TimeError::Empty => write!(f, "至少需要一个时间点，例如 21:31"),
        }
    }
}

impl std::error::Error for TimeError {}

/// 把一批时间串解析成排序去重后的时间点。
///
/// 对应 Python 版既能吃 `"21:31, 7:30"` 也能吃 `["21:31", "7:30"]` 的两种输入。
/// 空项跳过；一项都没有时报 [`TimeError::Empty`]。
pub fn parse_time_list<S: AsRef<str>>(
    raw: impl IntoIterator<Item = S>,
) -> Result<Vec<Clock>, TimeError> {
    let mut times = Vec::new();
    for chunk in raw {
        let chunk = chunk.as_ref();
        if chunk.trim().is_empty() {
            continue;
        }
        times.push(Clock::parse(chunk)?);
    }
    if times.is_empty() {
        return Err(TimeError::Empty);
    }
    times.sort();
    times.dedup();
    Ok(times)
}

/// 把 `"21:31, 7:30:05"` 解析成排序去重后的时间点。空串视为「没有时间点」并报错，
/// 与 Python 版一致（调用方负责在「允许为空」的场景先判空）。
pub fn parse_times(raw: &str) -> Result<Vec<Clock>, TimeError> {
    parse_time_list(raw.split(','))
}

/// 严格晚于 `now` 的下一个执行时间点；`times` 为空时报错。
pub fn next_fire(times: &[Clock], now: NaiveDateTime) -> Result<NaiveDateTime, TimeError> {
    times
        .iter()
        .map(|clock| clock.next_after(now))
        .min()
        .ok_or(TimeError::Empty)
}

/// 剩余秒数 → 「5 小时 39 分」，与界面文案一致。
pub fn humanize_delta(seconds: i64) -> String {
    let total = seconds.max(0);
    if total < 60 {
        return format!("{total} 秒");
    }
    let minutes = total / 60;
    let secs = total % 60;
    if minutes < 60 {
        return if secs == 0 {
            format!("{minutes} 分")
        } else {
            format!("{minutes} 分 {secs} 秒")
        };
    }
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours < 24 {
        return if mins == 0 {
            format!("{hours} 小时")
        } else {
            format!("{hours} 小时 {mins} 分")
        };
    }
    let days = hours / 24;
    let hrs = hours % 24;
    if hrs == 0 {
        format!("{days} 天")
    } else {
        format!("{days} 天 {hrs} 小时")
    }
}

/// 距离下一次执行的剩余秒数（已过期/相等时返回 0），供状态栏倒计时用。
pub fn seconds_until(target: NaiveDateTime, now: NaiveDateTime) -> i64 {
    (target - now).num_seconds().max(0)
}

/// 秒级时间戳取整，方便日志与 UI 对齐（去掉毫秒抖动）。
pub fn now_floor_second(now: NaiveDateTime) -> NaiveDateTime {
    now.with_nanosecond(0).unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn 排序去重并补齐秒() {
        let got = parse_times("21:31, 7:30:05").unwrap();
        assert_eq!(got.iter().map(|c| c.to_string()).collect::<Vec<_>>(), ["07:30:05", "21:31:00"]);
    }

    #[test]
    fn 允许一位小时() {
        assert_eq!(parse_times("7:30").unwrap()[0].to_string(), "07:30:00");
    }

    #[test]
    fn 完全相同的时间点只留一个() {
        assert_eq!(parse_times("21:31, 21:31:00").unwrap().len(), 1);
    }

    #[test]
    fn 越界报错() {
        assert!(matches!(parse_times("25:00"), Err(TimeError::OutOfRange(_))));
        assert!(matches!(parse_times("07:60"), Err(TimeError::OutOfRange(_))));
    }

    #[test]
    fn 格式错误报错() {
        assert!(matches!(parse_times("abc"), Err(TimeError::BadFormat(_))));
        assert!(matches!(parse_times("07"), Err(TimeError::BadFormat(_))));
    }

    #[test]
    fn 空输入报错() {
        assert!(matches!(parse_times("  "), Err(TimeError::Empty)));
    }

    #[test]
    fn 下一次触发_当天() {
        let times = parse_times("07:30, 21:31").unwrap();
        let now = at(2026, 11, 11, 16, 0);
        assert_eq!(next_fire(&times, now).unwrap(), at(2026, 11, 11, 21, 31));
    }

    #[test]
    fn 下一次触发_跨天() {
        let times = parse_times("07:30, 21:31").unwrap();
        let now = at(2026, 11, 11, 22, 0);
        assert_eq!(next_fire(&times, now).unwrap(), at(2026, 11, 12, 7, 30));
    }

    #[test]
    fn 恰好等于当前时刻要顺延到明天() {
        let times = parse_times("21:31").unwrap();
        let now = at(2026, 11, 11, 21, 31);
        assert_eq!(next_fire(&times, now).unwrap(), at(2026, 11, 12, 21, 31));
    }

    #[test]
    fn 中文时长() {
        assert_eq!(humanize_delta(0), "0 秒");
        assert_eq!(humanize_delta(59), "59 秒");
        assert_eq!(humanize_delta(60), "1 分");
        assert_eq!(humanize_delta(90), "1 分 30 秒");
        assert_eq!(humanize_delta(3600), "1 小时");
        assert_eq!(humanize_delta(3661), "1 小时 1 分");
        assert_eq!(humanize_delta(86_400), "1 天");
        assert_eq!(humanize_delta(90_000), "1 天 1 小时");
        assert_eq!(humanize_delta(-5), "0 秒");
    }
}
