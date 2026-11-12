//! 教学平台（bjmf.k8n.cn）的 HTTP 与 HTML 解析。
//!
//! 对应 Python 的 `utils/user_info.py` + `utils/attendance.py`。
//! 两件事分开写：**纯解析函数**（吃 HTML 字符串，好测）与**一个薄 HTTP 包装**。
//! 这样页面结构变化时，只需要改解析函数并补 fixture，不必起服务器。
//!
//! 行为对齐的细节：
//! - 状态字符串与 Python 完全一致（`success` / `already_signed` / `not_started` /
//!   `no_sign_in` / `error` / `skip`），双跑期可以直接 diff 日志。
//! - `no_sign_in` 判定依赖「列表页拿得到、但页面上没有进行中的签到」；
//!   如果两个模块的列表页都没拿到，属于网络/凭证问题，返回 `error` 而不是 `no_sign_in`。
//! - 只处理第一个签到项就返回 —— 这是 Python 版的实际行为（循环里直接 return），
//!   保持不动，避免一次运行里对同一活动重复提交。

use std::collections::BTreeMap;
use std::time::Duration;

use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::Student;

/// 线上站点根地址。
pub const DEFAULT_BASE: &str = "https://bjmf.k8n.cn";
/// 桌面端 / 网页端的 UA，与 Python 版逐字一致（站点对 UA 敏感，改了容易掉登录）。
pub const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; Linux; Android 9;) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Firefox/92.0  WeChat/x86_64 Weixin NetType/4G Language/zh_CN ABI/x86_64";
const TIMEOUT: Duration = Duration::from_secs(10);

/// 一次签到的结果。字符串形式与 Python 版一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignStatus {
    /// 本次提交成功。
    Success,
    /// 之前已经签过。
    AlreadySigned,
    /// 到了页面但签到还没开始。
    NotStarted,
    /// 今天没有进行中的签到。
    NoSignIn,
    /// 网络或凭证问题。
    Error,
    /// 主动跳过（例如拿不到用户信息）。
    Skip,
}

impl SignStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SignStatus::Success => "success",
            SignStatus::AlreadySigned => "already_signed",
            SignStatus::NotStarted => "not_started",
            SignStatus::NoSignIn => "no_sign_in",
            SignStatus::Error => "error",
            SignStatus::Skip => "skip",
        }
    }

    /// 需要人工介入的硬失败（对应 Python 的 `HARD_FAILURE_STATUS`）。
    pub fn is_hard_failure(self) -> bool {
        matches!(self, SignStatus::Skip | SignStatus::Error)
    }

    /// 汇总通知里算作成功的状态（已签到也算成功）。
    pub fn is_ok_for_summary(self) -> bool {
        matches!(self, SignStatus::Success | SignStatus::AlreadySigned)
    }

    /// 日志里用的中文短语。
    pub fn label(self) -> &'static str {
        match self {
            SignStatus::Success => "本次签到成功",
            SignStatus::AlreadySigned => "已签到，无需重复",
            SignStatus::NotStarted => "签到未开始",
            SignStatus::NoSignIn => "没有进行中的签到",
            SignStatus::Error => "执行失败",
            SignStatus::Skip => "已跳过",
        }
    }
}

/// 一个账号这次执行的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignOutcome {
    pub name: String,
    pub status: SignStatus,
    /// 给日志面板的一行说明（不含时间戳）。
    pub detail: String,
}

impl SignOutcome {
    pub fn new(name: impl Into<String>, status: SignStatus, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
        }
    }
}

/// 从班级/个人页读出来的资料。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub name: Option<String>,
    pub class_id: Option<String>,
    pub class_name: Option<String>,
    pub class_code: Option<String>,
}

#[derive(Debug, Error)]
pub enum SiteError {
    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),
    #[error("页面结构无法解析：{0}")]
    Parse(String),
}

// ==========================================================================
// 纯解析：不碰网络，全部有单测
// ==========================================================================

/// 抽出页面里的 `var gconfig={...}`，返回若干 `key:'value'` 键值对。
///
/// 站点把用户名、班级名、班级 ID 都塞在这一段里；值一律单引号包裹。
pub fn parse_gconfig(html: &str) -> BTreeMap<String, String> {
    let block = match Regex::new(r"var gconfig=\{([^}]+)\}").unwrap().captures(html) {
        Some(caps) => caps.get(1).map(|m| m.as_str()).unwrap_or(""),
        None => return BTreeMap::new(),
    };
    let pair = Regex::new(r"(\w+)\s*:\s*'([^']*)'").unwrap();
    pair.captures_iter(block)
        .filter_map(|caps| {
            Some((
                caps.get(1)?.as_str().to_string(),
                caps.get(2)?.as_str().to_string(),
            ))
        })
        .collect()
}

/// 个人中心页里的姓名。
pub fn parse_user_name(html: &str) -> Option<String> {
    parse_gconfig(html).get("uname").cloned()
}

/// 首页里 `a[href]` 指向的课程 ID 与课程名。
pub fn parse_course_link(html: &str) -> Option<(String, String)> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("a[href]").unwrap();
    let course = Regex::new(r"/student/course/(\d+)").unwrap();
    for element in doc.select(&selector) {
        let href = element.value().attr("href").unwrap_or("");
        if let Some(caps) = course.captures(href) {
            let id = caps.get(1).map(|m| m.as_str().to_string())?;
            let title = element.text().collect::<String>().trim().to_string();
            return Some((id, title));
        }
    }
    None
}

/// 卡片里的「班级码 XXXX」。
pub fn parse_class_code(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("div.card").unwrap();
    let pattern = Regex::new(r"班级码\s*([A-Z0-9]{4,10})").unwrap();
    for card in doc.select(&selector) {
        let text = card.text().collect::<Vec<_>>().join(" ");
        if let Some(caps) = pattern.captures(&text) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }
    }
    None
}

/// 一个签到项：活动 ID 与它所属的模块（`punchs` / `daka` / `punchw` …）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PunchItem {
    pub id: String,
    pub module: String,
}

/// 从列表页里找出所有签到项。
///
/// `page_module` 是**这个列表页属于哪个模块**（`punchs` / `daka`）：页面上 `punchcard_<id>`
/// 这类 DOM id 不带模块信息，只能靠调用方告诉它。`current_url` 用于识别
/// 「列表页被直接重定向到签到页」的情况，URL 里的模块优先。
///
/// 三条策略与 Python 一致，且**先去重再补**：同一个 id 只保留第一次看到的模块。
pub fn parse_punch_items(
    html: &str,
    current_url: Option<&str>,
    page_module: &str,
) -> Vec<PunchItem> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut ordered: Vec<PunchItem> = Vec::new();

    let mut push = |id: String, module: String, ordered: &mut Vec<PunchItem>| {
        if seen.contains_key(&id) {
            return;
        }
        seen.insert(id.clone(), module.clone());
        ordered.push(PunchItem { id, module });
    };

    let id_attr = Regex::new(r"punchcard_(\d+)").unwrap();
    for caps in id_attr.captures_iter(html) {
        let id = caps.get(1).unwrap().as_str().to_string();
        push(id, page_module.to_string(), &mut ordered);
    }

    let link = Regex::new(r"/student/(punch\w+|daka)/course/\d+/(\d+)").unwrap();
    for caps in link.captures_iter(html) {
        let module = caps.get(1).unwrap().as_str().to_string();
        let id = caps.get(2).unwrap().as_str().to_string();
        push(id, module, &mut ordered);
    }

    if let Some(url) = current_url {
        if let Some(caps) = link.captures(url) {
            let module = caps.get(1).unwrap().as_str().to_string();
            let id = caps.get(2).unwrap().as_str().to_string();
            push(id, module, &mut ordered);
        }
    }

    ordered
}

/// 提交签到后页面 `div#title` 的文字。
pub fn parse_title_text(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("div#title").unwrap();
    doc.select(&selector)
        .next()
        .map(|element| element.text().collect::<String>().trim().to_string())
}

/// 列表页上「已签到」的横幅（两种 class 都试）。
pub fn parse_signed_banner(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    for selector in ["div.punch-success-info", "div.punch-status"] {
        let parsed = Selector::parse(selector).unwrap();
        if let Some(element) = doc.select(&parsed).next() {
            let text = element.text().collect::<String>().trim().to_string();
            if text.contains("已签到") {
                return Some(text);
            }
        }
    }
    None
}

/// 由 `div#title` 的文字判定状态。
pub fn status_from_title(title: &str) -> SignStatus {
    if title.contains("已签到") {
        SignStatus::AlreadySigned
    } else if title.contains("未开始") {
        SignStatus::NotStarted
    } else {
        SignStatus::Success
    }
}

/// 登录页里的二维码图片地址。
pub fn extract_qr_url(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let container = Selector::parse("div#qrcode img").unwrap();
    if let Some(image) = doc.select(&container).next() {
        if let Some(src) = image.value().attr("src") {
            if !src.trim().is_empty() {
                return Some(src.to_string());
            }
        }
    }
    let any_img = Selector::parse("img").unwrap();
    for image in doc.select(&any_img) {
        let src = image.value().attr("src").unwrap_or("");
        if src.contains("ticket=") {
            return Some(src.to_string());
        }
    }
    None
}

/// 从 Cookie 串里挑出真正代表登录态的那一段。
///
/// 名字形如 `remember_student_<站点哈希>=<值>`，哈希随学校变化，所以只按前缀取；
/// 取不到（例如只有 `PHPSESSID`）就整串原样发送。
pub fn login_cookie_part(cookie: &str) -> String {
    let pattern = Regex::new(r"remember_student_[^;]+").unwrap();
    match pattern.find(cookie) {
        Some(found) => found.as_str().to_string(),
        None => cookie.trim().to_string(),
    }
}

/// 从 Cookie 名值对里挑登录态（扫码换 Cookie 时用）。
///
/// 丢掉无用的 `s`；优先 `remember_student_*`，否则退回第一个，保证 `PHPSESSID` 也能用。
pub fn pick_login_cookie(pairs: &[(String, String)]) -> Option<String> {
    let filtered: Vec<&(String, String)> = pairs.iter().filter(|(key, _)| key != "s").collect();
    if let Some((key, value)) = filtered
        .iter()
        .find(|(key, _)| key.starts_with("remember_student_"))
    {
        return Some(format!("{key}={value}"));
    }
    filtered
        .first()
        .map(|(key, value)| format!("{key}={value}"))
}

/// 扫码成功后的跳转地址 → 换取 Cookie 的地址。
pub fn build_uidlogin_url(redirect_url: &str) -> Result<String, SiteError> {
    build_uidlogin_url_from(UIDLOGIN_URL, redirect_url)
}

/// 同上，但允许指定换 Cookie 的入口地址（测试时指向本地 mock）。
pub fn build_uidlogin_url_from(uid_base: &str, redirect_url: &str) -> Result<String, SiteError> {
    let Some((_, query)) = redirect_url.split_once('?') else {
        return Err(SiteError::Parse(
            "登录跳转地址缺少参数，无法换取 Cookie".to_string(),
        ));
    };
    Ok(format!("{uid_base}{query}"))
}

pub const UIDLOGIN_URL: &str = "https://bj.k8n.cn/student/uidlogin?";

// ==========================================================================
// HTTP
// ==========================================================================

/// 读取姓名与班级（个人中心 + 首页）。两个页面各自尽力，拿不到就留空。
///
/// 抽成自由函数，是因为扫码登录（`qr`）也要用同一段逻辑，
/// 但它自己维护会话，不适合复用 [`Site`] 实例。
pub fn fetch_profile(
    client: &reqwest::blocking::Client,
    base: &str,
    cookie: &str,
) -> Result<Profile, SiteError> {
    let base = base.trim_end_matches('/');
    let cookie = login_cookie_part(cookie);
    let mut profile = Profile::default();

    let my = client
        .get(format!("{base}/student/my"))
        .header("User-Agent", DESKTOP_UA)
        .header("Cookie", &cookie)
        .send()?;
    if my.status().is_success() {
        let html = my.text()?;
        profile.name = parse_user_name(&html);
    }

    let home = client
        .get(format!("{base}/student"))
        .header("User-Agent", DESKTOP_UA)
        .header("Cookie", &cookie)
        .send()?;
    if home.status().is_success() {
        let html = home.text()?;
        let config = parse_gconfig(&html);
        profile.class_name = config.get("cname").cloned();
        profile.class_id = config.get("id").cloned();
        if let Some((id, name)) = parse_course_link(&html) {
            profile.class_id.get_or_insert(id);
            if profile.class_name.is_none() {
                profile.class_name = Some(name);
            }
        }
        profile.class_code = parse_class_code(&html);
    }

    Ok(profile)
}

/// 一个最小可用的站点客户端。`base` 可注入，测试时指向本地 mock。
pub struct Site {
    base: String,
    client: reqwest::blocking::Client,
}

impl Site {
    pub fn new(base: impl Into<String>) -> Result<Self, SiteError> {
        let client = reqwest::blocking::Client::builder()
            .cookie_store(true)
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn with_default_base() -> Result<Self, SiteError> {
        Self::new(DEFAULT_BASE)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// 读取姓名与班级。任何一个页面拿不到都不算失败，字段留空由调用方兜底。
    pub fn fetch_profile(&self, cookie: &str) -> Result<Profile, SiteError> {
        fetch_profile(&self.client, &self.base, cookie)
    }

    /// 跑一次签到。`student` 提供 Cookie、班级与坐标。
    pub fn sign_in(&self, student: &Student) -> Result<SignOutcome, SiteError> {
        self.sign_in_with_class(student, student.class.trim())
    }

    /// 同上，但班级 ID 由调用方给定（调用方可能刚从个人页读到更准的班级 ID）。
    pub fn sign_in_with_class(
        &self,
        student: &Student,
        class_id: &str,
    ) -> Result<SignOutcome, SiteError> {
        let name = student.name.clone();
        let cookie = login_cookie_part(&student.cookie);

        // 1) 预热：触发 remember_me 自动登录，失败不阻断
        let _ = self
            .client
            .get(self.url("/student/my"))
            .header("User-Agent", DESKTOP_UA)
            .header("Cookie", &cookie)
            .send();

        // 2) 探测列表页，合并签到项
        let class_id = class_id.trim();
        let mut items: Vec<PunchItem> = Vec::new();
        let mut last_html: Option<String> = None;
        let mut last_status_ok = false;

        for module in ["punchs", "daka"] {
            let list_url = self.url(&format!("/student/course/{class_id}/{module}"));
            let response = self
                .client
                .get(&list_url)
                .header("User-Agent", DESKTOP_UA)
                .header("Cookie", &cookie)
                .header(
                    "Referer",
                    self.url(&format!("/student/course/{class_id}")),
                )
                .send();

            let Ok(response) = response else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let final_url = response.url().to_string();
            let html = response.text()?;
            last_status_ok = true;
            let found = parse_punch_items(&html, Some(&final_url), module);
            for item in found {
                if !items.iter().any(|existing| existing.id == item.id) {
                    items.push(item);
                }
            }
            last_html = Some(html);
        }

        // 3) 没有签到项：区分「今天没有」与「根本没拿到页面」
        if items.is_empty() {
            let Some(html) = last_html else {
                return Ok(SignOutcome::new(
                    name,
                    SignStatus::Error,
                    "所有签到列表页请求失败，无法判断签到状态（请检查网络或 Cookie）",
                ));
            };
            if !last_status_ok {
                return Ok(SignOutcome::new(name, SignStatus::Error, "列表页返回异常"));
            }
            if let Some(banner) = parse_signed_banner(&html) {
                return Ok(SignOutcome::new(
                    name,
                    SignStatus::AlreadySigned,
                    format!("检测到已完成签到：{banner}"),
                ));
            }
            return Ok(SignOutcome::new(
                name,
                SignStatus::NoSignIn,
                "未找到进行中的签到（不在签到时间内）",
            ));
        }

        // 4) 提交第一个签到项（与 Python 行为一致）
        let item = &items[0];
        let url = self.url(&format!(
            "/student/{}/course/{}/{}",
            item.module, class_id, item.id
        ));
        let form = [
            ("id", item.id.as_str()),
            ("lat", student.lat.as_str()),
            ("lng", student.lng.as_str()),
            ("acc", student.acc.as_str()),
            ("res", ""),
            ("gps_addr", ""),
        ];
        let response = self
            .client
            .post(&url)
            .header("User-Agent", DESKTOP_UA)
            .header("Cookie", &cookie)
            .header(
                "Referer",
                self.url(&format!(
                    "/student/course/{class_id}/{}",
                    item.module
                )),
            )
            .form(&form)
            .send()?;

        if !response.status().is_success() {
            return Ok(SignOutcome::new(
                name,
                SignStatus::Error,
                format!("请求失败，状态码 {}", response.status().as_u16()),
            ));
        }

        let html = response.text()?;
        let detail = match parse_title_text(&html) {
            Some(title) => title,
            None => "签到请求成功（页面未返回状态文字）".to_string(),
        };
        let status = parse_title_text(&html)
            .map(|title| status_from_title(&title))
            .unwrap_or(SignStatus::Success);
        Ok(SignOutcome::new(name, status, detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MY: &str = include_str!("../tests/fixtures/student_my.html");
    const CLASS: &str = include_str!("../tests/fixtures/student_class.html");
    const LIST: &str = include_str!("../tests/fixtures/list_punchs.html");
    const DAKA: &str = include_str!("../tests/fixtures/list_daka.html");
    const LIST_SIGNED: &str = include_str!("../tests/fixtures/list_already_signed.html");
    const POST_OK: &str = include_str!("../tests/fixtures/post_success.html");
    const POST_ALREADY: &str = include_str!("../tests/fixtures/post_already_signed.html");
    const POST_NOT_STARTED: &str = include_str!("../tests/fixtures/post_not_started.html");
    const LOGIN: &str = include_str!("../tests/fixtures/login_qr.html");

    #[test]
    fn 解析_gconfig_里的用户名() {
        assert_eq!(parse_user_name(MY).as_deref(), Some("张明远"));
    }

    #[test]
    fn 解析_gconfig_里的班级字段() {
        let config = parse_gconfig(CLASS);
        assert_eq!(config.get("cname").map(String::as_str), Some("2023 级计算机 1 班"));
        assert_eq!(config.get("id").map(String::as_str), Some("2000001"));
    }

    #[test]
    fn 没有_gconfig_时返回空表() {
        assert!(parse_gconfig("<html></html>").is_empty());
    }

    #[test]
    fn 从链接里取课程_id_与名字() {
        let (id, name) = parse_course_link(CLASS).unwrap();
        assert_eq!(id, "2000001");
        assert_eq!(name, "2023 级计算机 1 班");
    }

    #[test]
    fn 解析班级码() {
        assert_eq!(parse_class_code(CLASS).as_deref(), Some("A1B2C3"));
    }

    #[test]
    fn 列表页能认出本模块的签到项() {
        let items = parse_punch_items(LIST, None, "punchs");
        assert_eq!(
            items,
            vec![PunchItem {
                id: "88213".into(),
                module: "punchs".into()
            }]
        );

        let daka = parse_punch_items(DAKA, None, "daka");
        assert_eq!(
            daka,
            vec![PunchItem {
                id: "88301".into(),
                module: "daka".into()
            }]
        );
    }

    #[test]
    fn 直接跳到签到页时从_url_里认() {
        let url = "https://bjmf.k8n.cn/student/punchw/course/2000001/90001";
        let items = parse_punch_items("<html></html>", Some(url), "punchs");
        assert_eq!(
            items,
            vec![PunchItem {
                id: "90001".into(),
                module: "punchw".into()
            }]
        );
    }

    #[test]
    fn 重复的签到项只留一个() {
        let html = r#"<a href="/student/punchs/course/1/100" id="punchcard_100"></a>"#;
        assert_eq!(parse_punch_items(html, None, "punchs").len(), 1);
    }

    #[test]
    fn 状态判定按标题文字() {
        assert_eq!(parse_title_text(POST_OK).as_deref(), Some("签到成功"));
        assert_eq!(
            parse_title_text(POST_OK).map(|t| status_from_title(&t)),
            Some(SignStatus::Success)
        );
        assert_eq!(
            parse_title_text(POST_ALREADY).map(|t| status_from_title(&t)),
            Some(SignStatus::AlreadySigned)
        );
        assert_eq!(
            parse_title_text(POST_NOT_STARTED).map(|t| status_from_title(&t)),
            Some(SignStatus::NotStarted)
        );
        // 文字之外的兜底
        assert_eq!(status_from_title("随便什么"), SignStatus::Success);
    }

    #[test]
    fn 列表页的已签到横幅() {
        assert!(parse_signed_banner(LIST_SIGNED).is_some());
        assert!(parse_signed_banner(LIST).is_none());
    }

    #[test]
    fn 登录页里找二维码地址() {
        let url = extract_qr_url(LOGIN).unwrap();
        assert!(url.contains("ticket=wx_ticket_9f8e7d"));
        assert!(extract_qr_url("<html></html>").is_none());
    }

    #[test]
    fn 登录_cookie_只取_remember_student_那段() {
        let raw = "s=abc; remember_student_9f8e=token123; other=1";
        assert_eq!(login_cookie_part(raw), "remember_student_9f8e=token123");
    }

    #[test]
    fn 没有_remember_student_时整串原样用() {
        assert_eq!(login_cookie_part("PHPSESSID=xyz"), "PHPSESSID=xyz");
    }

    #[test]
    fn 换_cookie_时优先_remember_student_并丢掉_s() {
        let pairs = vec![
            ("s".to_string(), "noise".to_string()),
            ("remember_student_abc".to_string(), "v".to_string()),
            ("PHPSESSID".to_string(), "p".to_string()),
        ];
        assert_eq!(pick_login_cookie(&pairs).as_deref(), Some("remember_student_abc=v"));
    }

    #[test]
    fn 换_cookie_时退回到第一个可用项() {
        let pairs = vec![("PHPSESSID".to_string(), "p".to_string())];
        assert_eq!(pick_login_cookie(&pairs).as_deref(), Some("PHPSESSID=p"));
        assert!(pick_login_cookie(&[]).is_none());
    }

    #[test]
    fn 跳转地址缺参数时报错() {
        assert!(build_uidlogin_url("https://bjmf.k8n.cn/done").is_err());
        let url = build_uidlogin_url("https://bjmf.k8n.cn/x?a=1&b=2").unwrap();
        assert_eq!(url, "https://bj.k8n.cn/student/uidlogin?a=1&b=2");
    }

    #[test]
    fn 状态字符串与_python_一致() {
        assert_eq!(SignStatus::Success.as_str(), "success");
        assert_eq!(SignStatus::AlreadySigned.as_str(), "already_signed");
        assert_eq!(SignStatus::NotStarted.as_str(), "not_started");
        assert_eq!(SignStatus::NoSignIn.as_str(), "no_sign_in");
        assert_eq!(SignStatus::Error.as_str(), "error");
        assert_eq!(SignStatus::Skip.as_str(), "skip");
    }

    #[test]
    fn 硬失败与汇总成功的口径() {
        assert!(SignStatus::Error.is_hard_failure());
        assert!(SignStatus::Skip.is_hard_failure());
        assert!(!SignStatus::NoSignIn.is_hard_failure());
        assert!(SignStatus::AlreadySigned.is_ok_for_summary());
        assert!(!SignStatus::NotStarted.is_ok_for_summary());
    }

    #[test]
    fn 标题缺失时归为成功() {
        assert_eq!(parse_title_text("<html></html>"), None);
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use httpmock::prelude::*;

    fn student() -> Student {
        Student {
            name: "张明远".into(),
            class: "2000001".into(),
            lat: "39.908720".into(),
            lng: "116.397450".into(),
            acc: "30".into(),
            cookie: "remember_student_abc=token".into(),
            ..Student::default()
        }
    }

    const MY: &str = include_str!("../tests/fixtures/student_my.html");
    const CLASS: &str = include_str!("../tests/fixtures/student_class.html");
    const LIST: &str = include_str!("../tests/fixtures/list_punchs.html");
    const LIST_SIGNED: &str = include_str!("../tests/fixtures/list_already_signed.html");
    const POST_OK: &str = include_str!("../tests/fixtures/post_success.html");
    const POST_ALREADY: &str = include_str!("../tests/fixtures/post_already_signed.html");
    const POST_NOT_STARTED: &str = include_str!("../tests/fixtures/post_not_started.html");

    #[test]
    fn 提交成功后返回_success() {
        let server = MockServer::start();
        let list = server.mock(|when, then| {
            when.method(GET).path("/student/course/2000001/punchs");
            then.status(200).body(LIST);
        });
        let post = server.mock(|when, then| {
            when.method(POST)
                .path("/student/punchs/course/2000001/88213")
                .body_contains("lat=39.908720")
                .body_contains("acc=30");
            then.status(200).body(POST_OK);
        });

        let site = Site::new(server.base_url()).unwrap();
        let outcome = site.sign_in(&student()).unwrap();

        assert_eq!(outcome.status, SignStatus::Success);
        assert_eq!(outcome.name, "张明远");
        list.assert();
        post.assert();
    }

    #[test]
    fn 已签到过返回_already_signed() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/student/course/2000001/punchs");
            then.status(200).body(LIST);
        });
        server.mock(|when, then| {
            when.method(POST).path("/student/punchs/course/2000001/88213");
            then.status(200).body(POST_ALREADY);
        });

        let site = Site::new(server.base_url()).unwrap();
        assert_eq!(site.sign_in(&student()).unwrap().status, SignStatus::AlreadySigned);
    }

    #[test]
    fn 未到时间返回_not_started() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/student/course/2000001/punchs");
            then.status(200).body(LIST);
        });
        server.mock(|when, then| {
            when.method(POST).path("/student/punchs/course/2000001/88213");
            then.status(200).body(POST_NOT_STARTED);
        });

        let site = Site::new(server.base_url()).unwrap();
        assert_eq!(site.sign_in(&student()).unwrap().status, SignStatus::NotStarted);
    }

    #[test]
    fn 列表页没有签到项且页面显示已签到() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/student/course/2000001/punchs");
            then.status(200).body(LIST_SIGNED);
        });

        let site = Site::new(server.base_url()).unwrap();
        let outcome = site.sign_in(&student()).unwrap();
        assert_eq!(outcome.status, SignStatus::AlreadySigned);
        assert!(outcome.detail.contains("已签到"));
    }

    #[test]
    fn 列表页没有签到项且没签过() {
        let server = MockServer::start();
        for module in ["punchs", "daka"] {
            let path = format!("/student/course/2000001/{module}");
            server.mock(move |when, then| {
                when.method(GET).path(path.as_str());
                then.status(200).body("<html><body>今天没有活动</body></html>");
            });
        }

        let site = Site::new(server.base_url()).unwrap();
        assert_eq!(site.sign_in(&student()).unwrap().status, SignStatus::NoSignIn);
    }

    #[test]
    fn 两个列表页都失败时是_error_而不是没签到() {
        let server = MockServer::start();
        // 不注册任何 mock：httpmock 对未匹配请求返回 404
        let site = Site::new(server.base_url()).unwrap();
        let outcome = site.sign_in(&student()).unwrap();
        assert_eq!(outcome.status, SignStatus::Error);
        assert!(outcome.detail.contains("Cookie"));
    }

    #[test]
    fn 提交返回非_200_时是_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/student/course/2000001/punchs");
            then.status(200).body(LIST);
        });
        server.mock(|when, then| {
            when.method(POST).path("/student/punchs/course/2000001/88213");
            then.status(500);
        });

        let site = Site::new(server.base_url()).unwrap();
        let outcome = site.sign_in(&student()).unwrap();
        assert_eq!(outcome.status, SignStatus::Error);
        assert!(outcome.detail.contains("500"));
    }

    #[test]
    fn 读取资料时姓名班级都拿到() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/student/my");
            then.status(200).body(MY);
        });
        server.mock(|when, then| {
            when.method(GET).path("/student");
            then.status(200).body(CLASS);
        });

        let site = Site::new(server.base_url()).unwrap();
        let profile = site.fetch_profile("remember_student_abc=token").unwrap();
        assert_eq!(profile.name.as_deref(), Some("张明远"));
        assert_eq!(profile.class_id.as_deref(), Some("2000001"));
        assert_eq!(profile.class_name.as_deref(), Some("2023 级计算机 1 班"));
        assert_eq!(profile.class_code.as_deref(), Some("A1B2C3"));
    }

    #[test]
    fn 读取资料时页面失败也不报错() {
        let server = MockServer::start();
        let site = Site::new(server.base_url()).unwrap();
        let profile = site.fetch_profile("PHPSESSID=x").unwrap();
        assert!(profile.name.is_none());
        assert!(profile.class_id.is_none());
    }
}
