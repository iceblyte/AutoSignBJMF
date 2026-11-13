//! 内置资源：logo 与图标。
//!
//! 用 `include_bytes!` 打进二进制，避免运行期依赖文件路径（便携版被挪走也不会丢图标）。
//! `img("logo.png")` 与图标都走这里。
//!
//! 图标不再自己维护 svg：统一用 gpui-kit 内置的 Lucide 图标集
//! （`IconName::Settings` 之类），本模块只负责拼上自己的 logo 与运行时图片，
//! 其余路径回退给 `gpui_kit::assets::AllAssets`。

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
    // 预缩到 64×64 的版本：界面里只显示 16–24px，用原图缩 30 倍会糊
    ("logo-64.png", include_bytes!("../assets/logo-64.png")),
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
        if let Some(bytes) = FILES
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes))
        {
            return Ok(Some(bytes));
        }

        // 回退到 gpui-kit 的内置图标集（Lucide，106 个）。
        // 之前没这一步，用 IconName::Settings 这类图标时路径找不到，画出来是空白。
        // 内置集里也没有时，按「没有这个资源」处理（返回 Ok(None)），不要冒泡成错误。
        match gpui_kit::assets::AllAssets.load(path) {
            Ok(bytes) => Ok(bytes),
            Err(_) => Ok(None),
        }
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
    fn 图标回退到内置图标集() {
        // 自己的清单里没有了，但 gpui-kit 的 Lucide 图标集要能兜住
        let bytes = Assets.load("icons/settings.svg").unwrap().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("<svg"), "内置图标读不到，设置页会画空白");
    }

    #[test]
    fn 不存在的路径返回_none() {
        assert!(Assets.load("nope.png").unwrap().is_none());
    }

    #[test]
    fn 列表按前缀过滤() {
        assert!(Assets.list("icons/").unwrap().is_empty(), "图标已交给内置集");
        assert!(Assets.list("nope/").unwrap().is_empty());
    }
}
