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
use crate::notify::Notifier;
use crate::site::{SignOutcome, SignStatus, Site};

/// 一轮执行的汇总。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub outcomes: Vec<SignOutcome>,
    /// 写回 `data.json` 时是否有改动（补班级）。
    pub config_changed: bool,
    /// 汇总通知是否真的发出去了。
    pub notified: bool,
    /// 下一轮重试要跑哪些账号（只放 `Error` 的下标）。
    ///
    /// 重试之所以按账号而不是按整轮：一个账号签成了、另一个失败了，
    /// 整轮重跑会把已经签成的再提交一遍（用户看到的就是「成功了还在签」）。
    pub retry: Vec<usize>,
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

    /// 还有需要重试的账号吗（决定调度器要不要再来一轮）。
    pub fn needs_retry(&self) -> bool {
        !self.retry.is_empty()
    }

    pub fn statuses(&self) -> Vec<(&str, &'static str)> {
        self.outcomes
            .iter()
            .map(|outcome| (outcome.name.as_str(), outcome.status.as_str()))
            .collect()
    }
}

/// 去重后的非空 Key 列表（保持原有顺序）。
///
/// 多个账号写同一个 Key 时只发一次：Server 酱 / Qmsg 酱按 Key 计费，
/// 重复发只会刷屏。空 Key 直接丢掉。
fn unique_keys<'a, I>(keys: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        let key = key.trim();
        if key.is_empty() || out.iter().any(|existing| existing == key) {
            continue;
        }
        out.push(key.to_string());
    }
    out
}

/// 执行器。`log` 是日志回调，界面把行流进日志面板，CLI 直接打印。
pub struct Runner<'a, L: FnMut(&str)> {
    site: &'a Site,
    notifier: &'a Notifier,
    config_path: PathBuf,
    /// 这些下标的账号本轮跳过（界面里单独暂停的账号）。
    skip: BTreeSet<usize>,
    /// 只跑这些下标（重试轮用：只重跑上一轮失败的账号）。
    only: Option<BTreeSet<usize>>,
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
            only: None,
            log,
        }
    }

    /// 指定本轮要跳过的账号下标（被单独暂停的账号）。
    pub fn with_skip(mut self, skip: impl IntoIterator<Item = usize>) -> Self {
        self.skip = skip.into_iter().collect();
        self
    }

    /// 只跑这些账号（重试轮用）。
    ///
    /// `None` = 全部账号（正常一轮）；`Some` = 只跑列出的下标，
    /// 已经成功的账号不会被重复提交。
    pub fn with_only(mut self, only: impl IntoIterator<Item = usize>) -> Self {
        self.only = Some(only.into_iter().collect());
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
            if let Some(only) = &self.only {
                if !only.contains(&index) {
                    continue;
                }
            }
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
            if outcome.status.is_retryable() {
                report.retry.push(index);
            }
            report.outcomes.push(outcome);
        }

        if report.config_changed {
            if let Err(error) = config.save(&self.config_path) {
                (self.log)(&format!("保存配置失败：{error}"));
            } else {
                (self.log)("已把探测到的班级 ID 写回配置");
            }
        }

        // 汇总通知：每个渠道、每个配了 Key 的账号都发一份。
        //
        // 以前只取「第一个非空的 Key」发一次 —— 多账号场景下其余 Key 永远收不到，
        // 用户配了好几个反而不知道哪个在生效。现在逐个发，重复的 Key 只发一次。
        if !report.outcomes.is_empty() {
            let message = crate::notify::summary_message(&report.outcomes);
            let wx_keys = unique_keys(
                config
                    .students
                    .iter()
                    .filter(|student| student.wx_enabled())
                    .map(|student| student.wx_key.as_str()),
            );
            let qq_keys = unique_keys(
                config
                    .students
                    .iter()
                    .filter(|student| student.qq_enabled())
                    .map(|student| student.qmsg_key.as_str()),
            );

            if wx_keys.is_empty() {
                (self.log)("未配置 WXKey 或微信通知已关闭，跳过微信汇总");
            }
            for key in wx_keys {
                match self.notifier.send_wx(&key, &message) {
                    Ok(true) => {
                        report.notified = true;
                        (self.log)("微信汇总通知已发送");
                    }
                    Ok(false) => (self.log)("微信汇总通知未发送（Key 为空）"),
                    Err(error) => (self.log)(&format!("微信汇总通知发送失败：{error}")),
                }
            }

            if qq_keys.is_empty() {
                (self.log)("未配置 QmsgKEY 或 QQ 通知已关闭，跳过 QQ 汇总");
            }
            for key in qq_keys {
                match self.notifier.send_qq(&key, &message) {
                    Ok(true) => {
                        report.notified = true;
                        (self.log)("QQ 汇总通知已发送");
                    }
                    Ok(false) => (self.log)("QQ 汇总通知未发送（Key 为空）"),
                    Err(error) => (self.log)(&format!("QQ 汇总通知发送失败：{error}")),
                }
            }
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

    /// 签到接口直接 500 的站点（用来造 Error）。
    fn failing_punch_site(server: &MockServer) -> Site {
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
            then.status(500).body("server error");
        });
        Site::new(server.base_url()).unwrap()
    }

    /// 一套健康 mock，其中签到接口的 mock 句柄返回出来，方便数提交次数。
    fn site_with_counted_punch(server: &MockServer) -> (Site, httpmock::Mock<'_>) {
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
        let punch = server.mock(|when, then| {
            when.method(POST).path("/student/punchs/course/2000001/88213");
            then.status(200).body(POST_OK);
        });
        (Site::new(server.base_url()).unwrap(), punch)
    }

    fn two_students_json() -> &'static str {
        r#"{"students":[
            {"name":"张明远","class":"2000001","lat":"39.9","lng":"116.4","acc":"30","cookie":"c"},
            {"name":"林可欣","class":"2000001","lat":"39.9","lng":"116.4","acc":"30","cookie":"c"}
        ]}"#
    }

    #[test]
    fn 签到接口出错才进重试队列() {
        let server = MockServer::start();
        let site = failing_punch_site(&server);
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config("retry-error.json", two_students_json());

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert_eq!(report.outcomes.len(), 2);
        assert!(report
            .outcomes
            .iter()
            .all(|outcome| outcome.status == SignStatus::Error));
        assert_eq!(report.retry, vec![0, 1], "两个账号都出错，都要重试");
        assert!(report.needs_retry());
    }

    #[test]
    fn 跳过不算需要重试() {
        let server = MockServer::start();
        // 不 mock /student/my：接口 404 → 拿不到姓名 → Skip
        let site = Site::new(server.base_url()).unwrap();
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        // Cookie 失效 → Skip：重试也还是同一个结果，不该占重试次数
        let path = temp_config(
            "retry-skip.json",
            r#"{"students":[{"name":"林可欣","class":"2000001","cookie":"bad"}]}"#,
        );

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert_eq!(report.outcomes[0].status, SignStatus::Skip);
        assert!(report.retry.is_empty(), "Skip 不该进重试队列");
        assert!(!report.needs_retry());
        // 但「给人看」的退出码口径不变：Skip 仍算失败
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn 重试轮只跑指定的账号() {
        let server = MockServer::start();
        let (site, punch) = site_with_counted_punch(&server);
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config("retry-only.json", two_students_json());

        // 只跑 1 号：不该碰 0 号（提交次数就是证据）
        let mut runner = Runner::new(&site, &notifier, &path, |_| {}).with_only([1]);
        let report = runner.run_once();

        assert_eq!(report.outcomes.len(), 1, "只有被点名的账号会被执行");
        assert_eq!(punch.hits(), 1, "只提交一次");
    }

    #[test]
    fn 重试不会把已经签成的账号再提交一遍() {
        let server = MockServer::start();
        let (site, punch) = site_with_counted_punch(&server);
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        let path = temp_config("retry-no-dup.json", two_students_json());

        // 第一轮：两个账号各提交一次
        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let first = runner.run_once();
        assert_eq!(punch.hits(), 2);
        assert!(first.retry.is_empty(), "都成功了就不该重试");

        // 模拟「只有 1 号需要重试」的那一轮
        let mut runner = Runner::new(&site, &notifier, &path, |_| {}).with_only([1]);
        let second = runner.run_once();
        assert_eq!(second.outcomes.len(), 1);
        assert_eq!(punch.hits(), 3, "重试只该多提交一次，不能再签 0 号");
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
    fn 多个渠道多个_key_都要发且重复_key_只发一次() {
        let server = MockServer::start();
        let site = healthy_site(&server);
        let wx_a = server.mock(|when, then| {
            when.method(POST).path("/WXA.send");
            then.status(200);
        });
        let wx_b = server.mock(|when, then| {
            when.method(POST).path("/WXB.send");
            then.status(200);
        });
        let qq_a = server.mock(|when, then| {
            when.method(POST).path("/send/QQA");
            then.status(200);
        });
        let notifier = Notifier::new(server.base_url(), server.base_url()).unwrap();
        // 两个账号各配一套；第二个账号的 WXKey 与第一个重复
        let path = temp_config(
            "multi-key.json",
            r#"{"students":[
                {"name":"张明远","class":"2000001","lat":"39.9","lng":"116.4","acc":"30","cookie":"c","WXKey":"WXA","QmsgKEY":"QQA"},
                {"name":"林可欣","class":"2000001","lat":"39.9","lng":"116.4","acc":"30","cookie":"c","WXKey":"WXA","QmsgKEY":""}
            ]}"#,
        );

        let mut runner = Runner::new(&site, &notifier, &path, |_| {});
        let report = runner.run_once();

        assert!(report.notified);
        assert_eq!(wx_a.hits(), 1, "重复的 Key 只发一次");
        assert_eq!(qq_a.hits(), 1, "QQ 渠道也要发");
        // 另一个不同 Key 的账号不存在，这里只是确保没有多发
        assert_eq!(wx_b.hits(), 0);
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
