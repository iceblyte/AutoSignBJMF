//! 消息推送（Server 酱 / Qmsg 酱）。
//!
//! 对应 `utils/notification.py`。拆成两层：**拼内容**（纯函数，可测）与**发出去**
//! （HTTP，用 mock 测）。密钥为空一律跳过，不报错 —— 用户可能只配一种渠道。
//!
//! 时间前缀用「本次执行的时间」，格式与 Python 一致（`HH:MM:SS` + 两个空格）。

use std::time::Duration;

use chrono::NaiveDateTime;
use thiserror::Error;

use crate::site::SignOutcome;

pub const WX_ENDPOINT: &str = "https://sctapi.ftqq.com";
pub const QQ_ENDPOINT: &str = "https://qmsg.zendee.cn";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("推送请求失败：{0}")]
    Http(#[from] reqwest::Error),
}

/// 一条推送内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// 标题（微信显示为大字，QQ 只有正文）
    pub title: String,
    /// 正文
    pub body: String,
}

/// Photoshop 的 `desp` 默认值，Python 里写死了 "123123"，这里沿用以免服务端判空。
pub const DEFAULT_DESP: &str = "123123";

fn hms(now: NaiveDateTime) -> String {
    now.format("%H:%M:%S").to_string()
}

/// 单条渠道消息（`sendQQmessage` / `wx_send` 共用的形状）。
pub fn simple_message(content: &str, now: NaiveDateTime) -> Message {
    let content = if content.trim().is_empty() {
        "签到成功！"
    } else {
        content
    };
    Message {
        title: format!("{}  {}", hms(now), content),
        body: DEFAULT_DESP.to_string(),
    }
}

/// 汇总消息：全成功 → 「全部签到成功」，否则「部分失败」。
///
/// 明细按「姓名成功 / 姓名失败」逐行给出，顺序沿用入参顺序，
/// 与 Python 的 `send_summary_notification` 一致（已签到算成功）。
pub fn summary_message(results: &[SignOutcome]) -> Message {
    let mut success = Vec::new();
    let mut failed = Vec::new();
    for outcome in results {
        if outcome.status.is_ok_for_summary() {
            success.push(format!("{}成功", outcome.name));
        } else {
            failed.push(format!("{}失败", outcome.name));
        }
    }
    let title = if failed.is_empty() {
        "全部签到成功"
    } else {
        "部分失败"
    };
    let body = [success.join("\n"), failed.join("\n")]
        .into_iter()
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Message {
        title: title.to_string(),
        body,
    }
}

/// 真正发请求的一端。`wx_base` / `qq_base` 可注入，测试时指向本地 mock。
pub struct Notifier {
    wx_base: String,
    qq_base: String,
    client: reqwest::blocking::Client,
}

impl Notifier {
    pub fn new(wx_base: impl Into<String>, qq_base: impl Into<String>) -> Result<Self, NotifyError> {
        Ok(Self {
            wx_base: wx_base.into().trim_end_matches('/').to_string(),
            qq_base: qq_base.into().trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(TIMEOUT)
                .build()?,
        })
    }

    pub fn production() -> Result<Self, NotifyError> {
        Self::new(WX_ENDPOINT, QQ_ENDPOINT)
    }

    /// Server 酱。`key` 为空直接返回 `Ok(false)`（表示跳过，不是失败）。
    pub fn send_wx(&self, key: &str, message: &Message) -> Result<bool, NotifyError> {
        if key.trim().is_empty() {
            return Ok(false);
        }
        let url = format!("{}/{}.send", self.wx_base, key.trim());
        let response = self
            .client
            .post(url)
            .form(&[("text", message.title.as_str()), ("desp", message.body.as_str())])
            .send()?;
        Ok(response.status().is_success())
    }

    /// Qmsg 酱。
    pub fn send_qq(&self, key: &str, message: &Message) -> Result<bool, NotifyError> {
        if key.trim().is_empty() {
            return Ok(false);
        }
        let url = format!("{}/send/{}", self.qq_base, key.trim());
        let response = self
            .client
            .post(url)
            .form(&[("msg", message.title.as_str())])
            .send()?;
        Ok(response.status().is_success())
    }

    /// 汇总推送：只用微信渠道，与 Python 一致（顶层 WXKey 已废弃，取第一个非空的）。
    pub fn send_summary(&self, wx_key: &str, results: &[SignOutcome]) -> Result<bool, NotifyError> {
        self.send_wx(wx_key, &summary_message(results))
    }
}

/// 从账号列表里挑第一个非空的微信 Key（Python 的同类逻辑）。
pub fn first_wx_key<'a, I>(keys: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    keys.into_iter().find(|key| !key.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::SignStatus;
    use chrono::{Local, NaiveDate};
    use httpmock::prelude::*;

    fn at(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 11)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    fn outcome(name: &str, status: SignStatus) -> SignOutcome {
        SignOutcome::new(name, status, "")
    }

    #[test]
    fn 单条消息带时间前缀与默认正文() {
        let message = simple_message("签到成功！", at(21, 31, 2));
        assert_eq!(message.title, "21:31:02  签到成功！");
        assert_eq!(message.body, DEFAULT_DESP);
    }

    #[test]
    fn 单条消息内容为空时用默认文案() {
        assert_eq!(simple_message("", at(7, 30, 0)).title, "07:30:00  签到成功！");
    }

    #[test]
    fn 汇总消息_全成功() {
        let results = vec![
            outcome("张明远", SignStatus::Success),
            outcome("林可欣", SignStatus::AlreadySigned),
        ];
        let message = summary_message(&results);
        assert_eq!(message.title, "全部签到成功");
        assert_eq!(message.body, "张明远成功\n林可欣成功");
    }

    #[test]
    fn 汇总消息_部分失败() {
        let results = vec![
            outcome("张明远", SignStatus::Success),
            outcome("林可欣", SignStatus::Error),
            outcome("陈子谦", SignStatus::Skip),
        ];
        let message = summary_message(&results);
        assert_eq!(message.title, "部分失败");
        assert_eq!(
            message.body,
            "张明远成功\n\n林可欣失败\n陈子谦失败"
        );
    }

    #[test]
    fn 没签到也算失败但不算硬失败之外的差异() {
        let results = vec![outcome("张明远", SignStatus::NoSignIn)];
        assert_eq!(summary_message(&results).title, "部分失败");
    }

    #[test]
    fn 微信推送表单字段正确() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/SCT123.send")
                .body_contains("text=21%3A31%3A02")
                .body_contains("desp=");
            then.status(200).body("{}");
        });

        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let sent = notifier
            .send_wx("SCT123", &simple_message("签到成功！", at(21, 31, 2)))
            .unwrap();
        assert!(sent);
        mock.assert();
    }

    #[test]
    fn qq_推送表单字段正确() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/send/QQKEY")
                .body_contains("msg=");
            then.status(200);
        });

        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        assert!(notifier
            .send_qq("QQKEY", &simple_message("签到成功！", at(7, 30, 0)))
            .unwrap());
        mock.assert();
    }

    #[test]
    fn 空_key_直接跳过且不发请求() {
        let server = MockServer::start();
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        assert!(!notifier.send_wx("  ", &simple_message("x", at(1, 0, 0))).unwrap());
        assert!(!notifier.send_qq("", &simple_message("x", at(1, 0, 0))).unwrap());
    }

    #[test]
    fn 服务端返回非_200_时报告未成功() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/BAD.send");
            then.status(500);
        });
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        assert!(!notifier
            .send_wx("BAD", &simple_message("x", at(1, 0, 0)))
            .unwrap());
    }

    #[test]
    fn 汇总推送走微信渠道() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/SCT9.send")
                .body_contains("text=");
            then.status(200);
        });
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let results = vec![outcome("张明远", SignStatus::Success)];
        assert!(notifier.send_summary("SCT9", &results).unwrap());
        mock.assert();
    }

    #[test]
    fn 取第一个非空微信_key() {
        assert_eq!(first_wx_key(["", "  ", "SCT1", "SCT2"]), Some("SCT1"));
        assert_eq!(first_wx_key(["", ""]), None);
    }

    #[test]
    fn 时间前缀用本地时区当天的时分秒() {
        let now = Local::now().naive_local();
        let message = simple_message("x", now);
        assert!(message.title.starts_with(&now.format("%H:%M:%S").to_string()));
    }
}
