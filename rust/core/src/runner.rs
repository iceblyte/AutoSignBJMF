//! 一轮签到：读配置 → 逐个账号签到 → 回写配置 → 发汇总通知。
//!
//! 对应 `utils/runner.py` 的 `run_once`，包括两个容易被忽略的行为：
//!
//! 1. **班级字段为空时会先探测再补**，补到就写回 `data.json`（下次不必再探测）
//! 2. **退出码**：只要有账号不是硬失败就算成功（0）；全是 `skip` / `error` 才返回 1。
//!    这样「今天没有签到」不会被当成失败。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::model::{Config, Student};
use crate::notify::{first_wx_key, Notifier};
use crate::site::{SignOutcome, SignStatus, Site};

/// 一轮执行的汇总。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub outcomes: Vec<SignOutcome>,
    /// 写回 `data.json` 时是否有改动（补班级）。
    pub config_changed: bool,
    /// 汇总通知是否真的发出去了。
    pub notified: bool,
}

impl Report {
    /// 0 = 至少有一个账号不是硬失败；1 = 全部硬失败。
    pub fn exit_code(&self) -> i32 {
        let hard = self
            .outcomes
            .iter()
            .filter(|outcome| outcome.status.is_hard_failure())
            .count();
        if !self.outcomes.is_empty() && hard == self.outcomes.len() {
            1
        } else {
            0
        }
    }

    pub fn statuses(&self) -> Vec<(&str, &'static str)> {
        self.outcomes
            .iter()
            .map(|outcome| (outcome.name.as_str(), outcome.status.as_str()))
            .collect()
    }
}

/// 执行器。`log` 是日志回调，界面把行流进日志面板，CLI 直接打印。
pub struct Runner<'a, L: FnMut(&str)> {
    site: &'a Site,
    notifier: &'a Notifier,
    config_path: PathBuf,
    /// 这些下标的账号本轮跳过（界面里单独暂停的账号）。
    skip: BTreeSet<usize>,
    log: L,
}

impl<'a, L: FnMut(&str)> Runner<'a, L> {
    pub fn new(
        site: &'a Site,
        notifier: &'a Notifier,
        config_path: impl Into<PathBuf>,
        log: L,
    ) -> Self {
        Self {
            site,
            notifier,
            config_path: config_path.into(),
            skip: BTreeSet::new(),
            log,
        }
    }

    /// 指定本轮要跳过的账号下标（被单独暂停的账号）。
    pub fn with_skip(mut self, skip: impl IntoIterator<Item = usize>) -> Self {
        self.skip = skip.into_iter().collect();
        self
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// 跑一轮。配置读不出来时按空配置处理（与 Python 一致，不抛错）。
    pub fn run_once(&mut self) -> Report {
        let mut config = Config::load(&self.config_path).unwrap_or_default();
        let mut report = Report::default();

        for index in 0..config.students.len() {
            if self.skip.contains(&index) {
                // 被单独暂停的账号：不进报告、不发通知，只留一行日志
                let name = config
                    .students
                    .get(index)
                    .map(|student| student.name.clone())
                    .unwrap_or_default();
                (self.log)(&format!("{name}：已暂停，本轮跳过"));
                continue;
            }
            let outcome = self.run_student(&mut config, index, &mut report.config_changed);
            (self.log)(&format!("{}：{}", outcome.name, outcome.detail));
            report.outcomes.push(outcome);
        }

        if report.config_changed {
            if let Err(error) = config.save(&self.config_path) {
                (self.log)(&format!("保存配置失败：{error}"));
            } else {
                (self.log)("已把探测到的班级 ID 写回配置");
            }
        }

        // 汇总通知：取第一个非空 WXKey（与 Python 相同）
        let wx_key = first_wx_key(config.students.iter().map(|s| s.wx_key.as_str()));
        match wx_key {
            Some(key) if !report.outcomes.is_empty() => {
                match self.notifier.send_summary(key, &report.outcomes) {
                    Ok(true) => report.notified = true,
                    Ok(false) => (self.log)("微信汇总通知未发送（Key 为空）"),
                    Err(error) => (self.log)(&format!("微信汇总通知发送失败：{error}")),
                }
            }
            _ => (self.log)("未配置 WXKey，跳过汇总通知"),
        }

        report
    }

    /// 单个账号：先确认凭证有效，再提交签到。
    fn run_student(&mut self, config: &mut Config, index: usize, changed: &mut bool) -> SignOutcome {
        // 克隆一份，避免后面借 config 做写回时冲突
        let student: Student = config.students[index].clone();
        let fallback_name = if student.name.trim().is_empty() {
            format!("账号{}", index + 1)
        } else {
            student.name.clone()
        };

        let profile = match self.site.fetch_profile(&student.cookie) {
            Ok(profile) => profile,
            Err(error) => {
                return SignOutcome::new(
                    fallback_name,
                    SignStatus::Error,
                    format!("读取用户信息失败：{error}"),
                );
            }
        };

        // 拿不到姓名 = Cookie 失效，直接跳过（不浪费一次签到请求）
        let Some(name) = profile.name.clone() else {
            return SignOutcome::new(
                fallback_name,
                SignStatus::Skip,
                "无法获取用户信息，可能是 Cookie 过期或无效，跳过该账号",
            );
        };

        // 班级：优先用页面上的，其次用配置里的；配置为空就补回去
        let mut class_id = profile.class_id.clone().unwrap_or_default();
        if class_id.trim().is_empty() {
            class_id = student.class.clone();
        }
        if student.class.trim().is_empty() && !class_id.trim().is_empty() {
            config.students[index].class = class_id.clone();
            *changed = true;
        }
        if class_id.trim().is_empty() {
            return SignOutcome::new(
                name,
                SignStatus::Skip,
                "无法获取班级 ID，跳过该账号（可在设置里手动填写）",
            );
        }

        let mut effective = student;
        effective.name = name.clone();
        effective.class = class_id.clone();

        match self.site.sign_in_with_class(&effective, &class_id) {
            Ok(mut outcome) => {
                outcome.name = name;
                outcome
            }
            Err(error) => SignOutcome::new(name, SignStatus::Error, format!("签到请求失败：{error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Config;
    use httpmock::prelude::*;
    use std::cell::RefCell;
    use std::fs;
    use std::rc::Rc;

    const MY: &str = include_str!("../tests/fixtures/student_my.html");
    const CLASS: &str = include_str!("../tests/fixtures/student_class.html");
    const LIST: &str = include_str!("../tests/fixtures/list_punchs.html");
    const POST_OK: &str = include_str!("../tests/fixtures/post_success.html");

    fn temp_config(name: &str, json: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("bjmf-core-runner");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, json).unwrap();
        path
    }

    /// 把"凭证有效 + 有签到 + 提交成功"这一套 mock 挂上。
    fn healthy_site(server: &MockServer) -> Site {
        server.mock(|when, then| {
            when.method(GET).path("/student/my");
            then.status(200).body(MY);
        });
        server.mock(|when, then| {
            when.method(GET).path("/student");
            then.status(200).body(CLASS);
        });
        server.mock(|when, then| {
            when.method(GET).path("/student/course/2000001/punchs");
            then.status(200).body(LIST);
        });
        server.mock(|when, then| {
            when.method(POST).path("/student/punchs/course/2000001/88213");
            then.status(200).body(POST_OK);
        });
        Site::new(server.base_url()).unwrap()
    }

    #[test]
    fn 全成功时退出码为0且发出汇总通知() {
        let server = MockServer::start();
        let site = healthy_site(&server);
        let notify_mock = server.mock(|when, then| {
            when.method(POST).path("/SCT1.send");
            then.status(200);
        });
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config(
            "ok.json",
            r#"{"students":[{"name":"张明远","class":"2000001","lat":"39.9","lng":"116.4","acc":"30","cookie":"c","WXKey":"SCT1"}]}"#,
        );

        let lines = Rc::new(RefCell::new(Vec::new()));
        let sink = lines.clone();
        let mut runner = Runner::new(&site, &notifier, &path, move |line| {
            sink.borrow_mut().push(line.to_string())
        });
        let report = runner.run_once();

        assert_eq!(report.exit_code(), 0);
        assert!(report.notified);
        assert_eq!(report.outcomes[0].status, SignStatus::Success);
        assert_eq!(
            report.statuses(),
            vec![("张明远", "success")]
        );
        notify_mock.assert();
        assert!(!lines.borrow().is_empty());
    }

    #[test]
    fn 凭证失效时跳过且退出码为1() {
        let server = MockServer::start();
        // /student/my 与 /student 都 404 → 拿不到姓名
        let site = Site::new(server.base_url()).unwrap();
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config(
            "expired.json",
            r#"{"students":[{"name":"林可欣","class":"2000001","cookie":"bad"}]}"#,
        );

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert_eq!(report.outcomes[0].status, SignStatus::Skip);
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn 班级为空时探测后写回配置文件() {
        let server = MockServer::start();
        let site = healthy_site(&server);
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config(
            "no-class.json",
            r#"{"students":[{"name":"张明远","class":"","lat":"39.9","lng":"116.4","acc":"30","cookie":"c"}]}"#,
        );

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert!(report.config_changed);
        let saved = Config::load(&path).unwrap();
        assert_eq!(saved.students[0].class, "2000001");
        assert_eq!(report.outcomes[0].status, SignStatus::Success);
    }

    #[test]
    fn 没有_wxkey_时不发通知() {
        let server = MockServer::start();
        let site = healthy_site(&server);
        let notify_mock = server.mock(|when, then| {
            when.method(POST).path_contains(".send");
            then.status(200);
        });
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config(
            "no-wx.json",
            r#"{"students":[{"name":"张明远","class":"2000001","lat":"39.9","lng":"116.4","acc":"30","cookie":"c","WXKey":""}]}"#,
        );

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert!(!report.notified);
        assert_eq!(notify_mock.hits(), 0);
    }

    #[test]
    fn 配置不存在时按空配置处理() {
        let server = MockServer::start();
        let site = Site::new(server.base_url()).unwrap();
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let dir = std::env::temp_dir().join("bjmf-core-runner");
        let path = dir.join("missing.json");
        let _ = fs::remove_file(&path);

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert!(report.outcomes.is_empty());
        assert_eq!(report.exit_code(), 0);
    }
}
