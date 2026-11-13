//! `data.json` 的读写。
//!
//! 契约来自仓库根目录的 `data.json.example`，字段名大小写**必须原样保留**
//! （`QmsgKEY` / `WXKey`），否则 Python 版读不到。
//!
//! 历史配置与手工编辑的文件经常缺字段，所以所有字段都 `#[serde(default)]`：
//! 少字段能读，多字段会被忽略但**不会丢** —— 保留 `extra` 以便原样写回。

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 一个账号。字段名与 `data.json` 一一对应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Student {
    pub name: String,
    pub class: String,
    pub lat: String,
    pub lng: String,
    /// 定位精度（米），旧配置里可能是数字，统一按字符串处理后再解析。
    pub acc: String,
    pub cookie: String,
    #[serde(rename = "QmsgKEY")]
    pub qmsg_key: String,
    #[serde(rename = "WXKey")]
    pub wx_key: String,
    /// 通知渠道是否启用。
    ///
    /// 旧配置没有这个字段（`None`），此时按「Key 是否为空」推断，行为和以前一致；
    /// 用户点过开关之后才会写进 `data.json`。关掉渠道不会删除 Key。
    #[serde(rename = "WXNoticeOn", skip_serializing_if = "Option::is_none")]
    pub wx_on: Option<bool>,
    #[serde(rename = "QmsgNoticeOn", skip_serializing_if = "Option::is_none")]
    pub qq_on: Option<bool>,
    /// 未识别的字段原样保留，避免写回时把用户手写的内容吃掉。
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Student {
    /// 微信通知是否启用。
    pub fn wx_enabled(&self) -> bool {
        self.wx_on.unwrap_or_else(|| !self.wx_key.trim().is_empty())
    }

    /// QQ 通知是否启用。
    pub fn qq_enabled(&self) -> bool {
        self.qq_on.unwrap_or_else(|| !self.qmsg_key.trim().is_empty())
    }

    /// 剩余信息是否完整到可以签到：有 Cookie、有坐标。
    pub fn is_ready(&self) -> bool {
        !self.cookie.trim().is_empty()
            && !self.lat.trim().is_empty()
            && !self.lng.trim().is_empty()
    }

    /// 界面上的副标题，与三端原型一致：`2023 级计算机 1 班 · 2000001`。
    pub fn subtitle(&self) -> String {
        match self.class.trim() {
            "" => "未填写班级".to_string(),
            class => class.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub students: Vec<Student>,
}

#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: std::io::Error },
    Json { path: PathBuf, source: serde_json::Error },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "读写 {} 失败：{}", path.display(), source)
            }
            ConfigError::Json { path, source } => {
                write!(f, "解析 {} 失败：{}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// 读取配置。文件不存在时返回空配置（与 Python 版行为一致，不报错）。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if text.trim().is_empty() {
            return Ok(Config::default());
        }
        serde_json::from_str(&text).map_err(|source| ConfigError::Json {
            path: path.to_path_buf(),
            source,
        })
    }

    /// 保存配置：先写临时文件再改名，避免写一半断电留下坏 JSON。
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let text = serde_json::to_string_pretty(self).expect("配置序列化不应失败");
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|source| ConfigError::Io {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "students": [
            {
                "name": "张明远",
                "class": "2023 级计算机 1 班",
                "lat": "39.908720",
                "lng": "116.397450",
                "acc": "30",
                "cookie": "remember_student=abc",
                "QmsgKEY": "",
                "WXKey": "SCT123"
            }
        ]
    }"#;

    #[test]
    fn 字段名大小写必须匹配_python() {
        let cfg: Config = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(cfg.students[0].qmsg_key, "");
        assert_eq!(cfg.students[0].wx_key, "SCT123");
    }

    #[test]
    fn 缺字段也能读() {
        let cfg: Config = serde_json::from_str(r#"{"students":[{"name":"A"}]}"#).unwrap();
        assert_eq!(cfg.students[0].name, "A");
        assert_eq!(cfg.students[0].acc, "");
        assert!(!cfg.students[0].is_ready());
    }

    #[test]
    fn 空文件视为空配置() {
        let dir = std::env::temp_dir().join("bjmf-core-test");
        let p = dir.join("empty.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&p, "   ").unwrap();
        assert!(Config::load(&p).unwrap().students.is_empty());
    }

    #[test]
    fn 通知开关缺省时按_key_是否为空推断() {
        // 旧配置没有 WXNoticeOn 字段
        let with_key: Student = serde_json::from_str(r#"{"name":"A","WXKey":"k"}"#).unwrap();
        assert!(with_key.wx_enabled(), "有 Key 且没开关字段时视为开启");

        let without: Student = serde_json::from_str(r#"{"name":"A"}"#).unwrap();
        assert!(!without.wx_enabled(), "没 Key 时视为关闭");
        assert!(!without.qq_enabled());
    }

    #[test]
    fn 关掉渠道不会丢掉_key() {
        let mut student: Student =
            serde_json::from_str(r#"{"name":"A","WXKey":"secret","QmsgKEY":"q"}"#).unwrap();
        student.wx_on = Some(false);

        let text = serde_json::to_string(&student).unwrap();
        let back: Student = serde_json::from_str(&text).unwrap();

        assert_eq!(back.wx_key, "secret", "关闭渠道不能删 Key");
        assert!(!back.wx_enabled(), "关闭状态要能读回来");
        assert!(back.qq_enabled(), "另一个渠道不受影响");
    }

    #[test]
    fn 未识别字段写回时保留() {
        let raw = r#"{"students":[{"name":"A","foo":"bar"}]}"#;
        let cfg: Config = serde_json::from_str(raw).unwrap();
        let out = serde_json::to_string(&cfg).unwrap();
        assert!(out.contains("\"foo\":\"bar\""));
    }

    #[test]
    fn 存取往返一致() {
        let dir = std::env::temp_dir().join("bjmf-core-test");
        let p = dir.join("roundtrip.json");
        let cfg: Config = serde_json::from_str(SAMPLE).unwrap();
        cfg.save(&p).unwrap();
        assert_eq!(Config::load(&p).unwrap(), cfg);
        let _ = fs::remove_file(&p);
    }
}
