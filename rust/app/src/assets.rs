//! 内置资源：logo 与图标。
//!
//! 用 `include_bytes!` 打进二进制，避免运行期依赖文件路径（便携版被挪走也不会丢图标）。
//! `img("logo.png")` 与 `svg().path("icons/pin.svg")` 都走这里。

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use gpui_kit::{AssetSource, Result, SharedString};

/// 运行期资源：扫码拿到的二维码图片写这里，`img("qr-1.png")` 就能显示。
fn runtime() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 存入一张运行时图片，返回可直接给 `img()` 的路径。
pub fn put_image(bytes: Vec<u8>) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let index = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let name = format!("qr-{index}.png");
    if let Ok(mut map) = runtime().lock() {
        map.insert(name.clone(), bytes);
    }
    name
}

/// 资源清单：路径 → 字节。
const FILES: &[(&str, &[u8])] = &[
    ("logo.png", include_bytes!("../assets/logo.png")),
    ("icons/pin.svg", include_bytes!("../assets/icons/pin.svg")),
    ("icons/gear.svg", include_bytes!("../assets/icons/gear.svg")),
    ("icons/bell.svg", include_bytes!("../assets/icons/bell.svg")),
    (
        "icons/database.svg",
        include_bytes!("../assets/icons/database.svg"),
    ),
    ("icons/info.svg", include_bytes!("../assets/icons/info.svg")),
    (
        "icons/person-add.svg",
        include_bytes!("../assets/icons/person-add.svg"),
    ),
    ("icons/warn.svg", include_bytes!("../assets/icons/warn.svg")),
];

/// 资源加载器。
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Ok(map) = runtime().lock() {
            if let Some(bytes) = map.get(path) {
                return Ok(Some(Cow::Owned(bytes.clone())));
            }
        }
        Ok(FILES
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(FILES
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_能读到且是_png() {
        let bytes = Assets.load("logo.png").unwrap().unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn 图标都是_svg() {
        for name in [
            "icons/pin.svg",
            "icons/gear.svg",
            "icons/bell.svg",
            "icons/database.svg",
            "icons/info.svg",
            "icons/person-add.svg",
            "icons/warn.svg",
        ] {
            let bytes = Assets.load(name).unwrap().unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.starts_with("<svg"), "{name} 不是 svg");
        }
    }

    #[test]
    fn 不存在的路径返回_none() {
        assert!(Assets.load("nope.png").unwrap().is_none());
    }

    #[test]
    fn 列表按前缀过滤() {
        let icons = Assets.list("icons/").unwrap();
        assert_eq!(icons.len(), 7);
        assert!(Assets.list("nope/").unwrap().is_empty());
    }
}
