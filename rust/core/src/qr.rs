//! 微信扫码登录。
//!
//! 对应 `desktop/qr_client.py` 与 `auto_add_user.py` 的流程：
//! 拉登录页 → 取二维码图片 → 轮询扫码状态 → 用跳转地址换 Cookie。
//!
//! 二维码图片以 `Vec<u8>`（PNG）返回，交给界面去解码显示；
//! **Cookie 明文不进日志**。

use std::time::Duration;

use serde_json::Value;
use thiserror::Error;

use crate::site::{
    build_uidlogin_url_from, extract_qr_url, pick_login_cookie, Profile, SiteError, UIDLOGIN_URL,
};

/// 扫码登录入口。
pub const DEFAULT_BASE: &str = "https://bjmf.k8n.cn";
pub const LOGIN_PATH: &str = "/weixin/qrlogin/student";

/// 手机微信 UA：站点按 UA 决定给不给二维码，改错会拿到 PC 页。
pub const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 10; SM-G981B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.162 Mobile Safari/537.36 MicroMessenger/7.0.10.1580(0x27000A50) Process/tools NetType/WIFI Language/zh_CN ABI/arm64";

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum QrError {
    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),
    #[error("登录页里没找到二维码图片，可能接口已变更")]
    NoQrImage,
    #[error("登录跳转地址缺少参数，无法换取 Cookie")]
    NoRedirectQuery,
    #[error("未获取到有效 cookie")]
    NoCookie,
    #[error(transparent)]
    Site(#[from] SiteError),
}

/// 一轮扫码会话。一个实例对应一张二维码。
pub struct QrLogin {
    base: String,
    /// 换 Cookie 的入口，默认线上地址，测试时可指向本地 mock。
    uid_base: String,
    client: reqwest::blocking::Client,
}

impl QrLogin {
    pub fn new(base: impl Into<String>) -> Result<Self, QrError> {
        Self::with_uid_base(base, UIDLOGIN_URL)
    }

    pub fn with_uid_base(
        base: impl Into<String>,
        uid_base: impl Into<String>,
    ) -> Result<Self, QrError> {
        let client = reqwest::blocking::Client::builder()
            .cookie_store(true)
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_string(),
            uid_base: uid_base.into(),
            client,
        })
    }

    pub fn production() -> Result<Self, QrError> {
        Self::new(DEFAULT_BASE)
    }

    fn login_url(&self) -> String {
        format!("{}{}", self.base, LOGIN_PATH)
    }

    fn send_get(&self, url: &str) -> Result<reqwest::blocking::Response, QrError> {
        Ok(self
            .client
            .get(url)
            .header("User-Agent", MOBILE_UA)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Referer", "https://wx.qq.com/")
            .header("X-Requested-With", "XMLHttpRequest")
            .send()?)
    }

    /// 取二维码图片（PNG 字节）。
    pub fn create(&self) -> Result<Vec<u8>, QrError> {
        let html = self.send_get(&self.login_url())?.text()?;
        let image_url = extract_qr_url(&html).ok_or(QrError::NoQrImage)?;
        let response = self.send_get(&image_url)?;
        let response = response.error_for_status()?;
        Ok(response.bytes()?.to_vec())
    }

    /// 轮询扫码结果。已确认时返回跳转地址。
    pub fn poll(&self) -> Result<Option<String>, QrError> {
        let url = format!("{}?op=checklogin", self.login_url());
        let response = self.send_get(&url)?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let Ok(data) = response.json::<Value>() else {
            return Ok(None);
        };
        let confirmed = match data.get("status") {
            Some(Value::Bool(flag)) => *flag,
            Some(Value::Number(number)) => number.as_i64().map(|n| n != 0).unwrap_or(false),
            Some(Value::String(text)) => text == "1" || text == "true",
            _ => false,
        };        let redirect = data
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|url| !url.is_empty());
        match (confirmed, redirect) {
            (true, Some(url)) => Ok(Some(url)),
            _ => Ok(None),
        }
    }

    /// 用跳转地址换登录 Cookie。
    pub fn exchange_cookie(&self, redirect_url: &str) -> Result<String, QrError> {
        let url =
            build_uidlogin_url_from(&self.uid_base, redirect_url).map_err(|_| QrError::NoRedirectQuery)?;

        // 不跟随跳转：要的就是这一跳响应里的 Set-Cookie。
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .cookie_store(true)
            .timeout(TIMEOUT)
            .build()?;
        let response = client
            .get(&url)
            .header("User-Agent", MOBILE_UA)
            .header("Referer", "https://wx.qq.com/")
            .send()?;

        let mut pairs: Vec<(String, String)> = response
            .cookies()
            .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
            .collect();
        if pairs.is_empty() {
            pairs = parse_set_cookie_headers(&response);
        }

        pick_login_cookie(&pairs).ok_or(QrError::NoCookie)
    }

    /// 用新 Cookie 拉一次资料，顺便验证 Cookie 可用。
    pub fn fetch_profile(&self, cookie: &str) -> Result<Profile, QrError> {
        Ok(crate::site::fetch_profile(&self.client, &self.base, cookie)?)
    }
}

/// 从 `Set-Cookie` 头里解析 `name=value`（`Response::cookies` 没拿到时的兜底）。
fn parse_set_cookie_headers(response: &reqwest::blocking::Response) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for header in response.headers().get_all("set-cookie") {
        let Ok(text) = header.to_str() else { continue };
        let first = text.split(';').next().unwrap_or("");
        if let Some((name, value)) = first.split_once('=') {
            pairs.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const LOGIN: &str = include_str!("../tests/fixtures/login_qr.html");

    #[test]
    fn 拉二维码返回图片字节() {
        let server = MockServer::start();
        let html = LOGIN.replace(
            "https://open.weixin.qq.com/connect/qrcode/0712abcdEFGH-ticket=wx_ticket_9f8e7d",
            &format!("{}/qrcode.png", server.base_url()),
        );
        server.mock(|when, then| {
            when.method(GET).path(LOGIN_PATH);
            then.status(200).body(html);
        });
        server.mock(|when, then| {
            when.method(GET).path("/qrcode.png");
            then.status(200)
                .header("content-type", "image/png")
                .body("PNGDATA");
        });

        let qr = QrLogin::new(server.base_url()).unwrap();
        assert_eq!(qr.create().unwrap(), b"PNGDATA");
    }

    #[test]
    fn 登录页没有二维码时报错() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path(LOGIN_PATH);
            then.status(200).body("<html>nothing</html>");
        });
        let qr = QrLogin::new(server.base_url()).unwrap();
        assert!(matches!(qr.create(), Err(QrError::NoQrImage)));
    }

    #[test]
    fn 未确认时轮询返回_none() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path(LOGIN_PATH).query_param("op", "checklogin");
            then.status(200).json_body(serde_json::json!({"status": 0}));
        });
        let qr = QrLogin::new(server.base_url()).unwrap();
        assert_eq!(qr.poll().unwrap(), None);
    }

    #[test]
    fn 确认后返回跳转地址() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path(LOGIN_PATH).query_param("op", "checklogin");
            then.status(200)
                .json_body(serde_json::json!({"status": 1, "url": "https://bjmf.k8n.cn/x?a=1"}));
        });
        let qr = QrLogin::new(server.base_url()).unwrap();
        assert_eq!(qr.poll().unwrap().as_deref(), Some("https://bjmf.k8n.cn/x?a=1"));
    }

    #[test]
    fn 换_cookie_时优先_remember_student() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/student/uidlogin");
            then.status(302)
                .header("set-cookie", "s=noise; Path=/")
                .header("set-cookie", "remember_student_9f8e=token123; Path=/");
        });

        let qr = QrLogin::with_uid_base(
            server.base_url(),
            format!("{}/student/uidlogin?", server.base_url()),
        )
        .unwrap();
        let cookie = qr.exchange_cookie("https://bjmf.k8n.cn/x?a=1&b=2").unwrap();
        assert_eq!(cookie, "remember_student_9f8e=token123");
        mock.assert();
    }

    #[test]
    fn 换_cookie_拿不到时是_no_cookie() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/student/uidlogin");
            then.status(200);
        });
        let qr = QrLogin::with_uid_base(
            server.base_url(),
            format!("{}/student/uidlogin?", server.base_url()),
        )
        .unwrap();
        assert!(matches!(
            qr.exchange_cookie("https://bjmf.k8n.cn/x?a=1"),
            Err(QrError::NoCookie)
        ));
    }

    #[test]
    fn 跳转地址缺参数时报错() {
        let server = MockServer::start();
        let qr = QrLogin::new(server.base_url()).unwrap();
        assert!(matches!(
            qr.exchange_cookie("https://bjmf.k8n.cn/done"),
            Err(QrError::NoRedirectQuery)
        ));
    }
}
