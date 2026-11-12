//! 设计令牌：与 `design/v2/proto.css` 的 `:root` / `[data-theme="dark"]` 一一对应。
//!
//! 取值必须**逐字**来自原型，不要在这里"微调"——原型是对外的契约，
//! 改色请先改 `design/v2/01-style.html`，再同步到这张表。
//!
//! 颜色用 `0xRRGGBB`，配合 `rgb()` 使用。

/// 一套配色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub surface: u32,
    pub surface_2: u32,
    pub surface_3: u32,
    pub line: u32,
    pub line_2: u32,
    pub text: u32,
    pub text_2: u32,
    pub text_3: u32,
    pub accent: u32,
    pub accent_h: u32,
    pub accent_soft: u32,
    pub accent_line: u32,
    pub ok: u32,
    pub ok_soft: u32,
    pub ok_line: u32,
    pub warn: u32,
    pub warn_soft: u32,
    pub warn_line: u32,
    pub bad: u32,
    pub bad_soft: u32,
    pub bad_line: u32,
    /// 关闭按钮悬停时的红（原型写死在 `.wt .wbtn.close:hover`）。
    pub close_hover: u32,
    /// 窗口在桌面背景上的一层（`.desk` 在深色下用 #0b0e12）。
    pub desk: u32,
}

pub const LIGHT: Palette = Palette {
    bg: 0xf6f7f9,
    surface: 0xffffff,
    surface_2: 0xfafbfc,
    surface_3: 0xf1f3f6,
    line: 0xe6e8ec,
    line_2: 0xd8dbe1,
    text: 0x131619,
    text_2: 0x545a63,
    text_3: 0x868d97,
    accent: 0x2563eb,
    accent_h: 0x1d4ed8,
    accent_soft: 0xeff4ff,
    accent_line: 0xcfdcfb,
    ok: 0x0d9488,
    ok_soft: 0xe9faf6,
    ok_line: 0xb9ebdf,
    warn: 0xb45309,
    warn_soft: 0xfff8ea,
    warn_line: 0xf3e0b6,
    bad: 0xb42318,
    bad_soft: 0xfef4f2,
    bad_line: 0xf5d2cc,
    close_hover: 0xc42b1c,
    desk: 0xf1f3f6,
};

pub const DARK: Palette = Palette {
    bg: 0x101317,
    surface: 0x191d23,
    surface_2: 0x1d2229,
    surface_3: 0x232830,
    line: 0x2b313a,
    line_2: 0x3a424d,
    text: 0xeef1f5,
    text_2: 0xa9b2bf,
    text_3: 0x7d8794,
    accent: 0x5b8cf7,
    accent_h: 0x7aa1f9,
    accent_soft: 0x17253c,
    accent_line: 0x2b4064,
    ok: 0x34d3bd,
    ok_soft: 0x0e2a26,
    ok_line: 0x1d4b44,
    warn: 0xf0b44a,
    warn_soft: 0x2a2113,
    warn_line: 0x4a3a1c,
    bad: 0xf08a80,
    bad_soft: 0x2c1614,
    bad_line: 0x4d2a26,
    close_hover: 0xc42b1c,
    desk: 0x0b0e12,
};

/// 主题模式。`System` 读 Windows 的浅色/深色设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    /// 根据"系统是否深色"解析出最终是否深色。
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => system_dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "浅色",
            ThemeMode::Dark => "深色",
            ThemeMode::System => "跟随系统",
        }
    }

    pub fn all() -> [ThemeMode; 3] {
        [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System]
    }
}

/// 读注册表里的系统主题：`AppsUseLightTheme = 0` 表示深色。
///
/// 读不到就当作浅色（Windows 默认），不要让主题影响启动。
pub fn system_is_dark() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    };

    let subkey: Vec<u16> = OsStr::new(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let value: Vec<u16> = OsStr::new("AppsUseLightTheme")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut key = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return false;
        }
        let mut data: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let mut kind: u32 = 0;
        let status = RegQueryValueExW(
            key,
            value.as_ptr(),
            std::ptr::null(),
            &mut kind,
            &mut data as *mut u32 as *mut u8,
            &mut size,
        );
        RegCloseKey(key);
        if status != 0 || kind != REG_DWORD {
            return false;
        }
        data == 0
    }
}

// ---------------- 尺寸（原型里的固定值） ----------------

/// 主窗口默认尺寸（`.win` 1160×800，画板 1240×860）。
pub const WINDOW_W: f32 = 1160.0;
pub const WINDOW_H: f32 = 800.0;
/// 自绘标题栏高度。
pub const TITLEBAR_H: f32 = 36.0;
/// 窗口按钮宽度（46×36 全高，直角）。
pub const WINDOW_BTN_W: f32 = 46.0;
/// 侧栏宽度。
pub const SIDEBAR_W: f32 = 264.0;
/// 状态栏高度。
pub const STATUSBAR_H: f32 = 38.0;
/// 面板标题行高度。
pub const PANEL_HEAD_H: f32 = 34.0;
/// 控件高度：exe 端统一 32（原型 `--ctl` 在 exe 分支为 32）。
pub const CTL_H: f32 = 32.0;
/// 小控件 / 小按钮高度。
pub const CTL_SM_H: f32 = 28.0;
pub const CTL_XS_H: f32 = 24.0;
/// 账号行高度。
pub const ACCOUNT_ROW_H: f32 = 46.0;
/// 设置窗口尺寸。
pub const SETTINGS_W: f32 = 860.0;
pub const SETTINGS_H: f32 = 560.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 浅色令牌与原型一致() {
        assert_eq!(LIGHT.surface, 0xffffff);
        assert_eq!(LIGHT.text, 0x131619);
        assert_eq!(LIGHT.accent, 0x2563eb);
        assert_eq!(LIGHT.ok, 0x0d9488);
        assert_eq!(LIGHT.line, 0xe6e8ec);
    }

    #[test]
    fn 深色令牌与原型一致() {
        assert_eq!(DARK.bg, 0x101317);
        assert_eq!(DARK.surface, 0x191d23);
        assert_eq!(DARK.text, 0xeef1f5);
        assert_eq!(DARK.accent, 0x5b8cf7);
        assert_eq!(DARK.desk, 0x0b0e12);
    }

    #[test]
    fn 关闭按钮悬停红两套一致() {
        assert_eq!(LIGHT.close_hover, 0xc42b1c);
        assert_eq!(DARK.close_hover, 0xc42b1c);
    }

    #[test]
    fn 主题模式解析() {
        assert!(!ThemeMode::Light.is_dark(true));
        assert!(ThemeMode::Dark.is_dark(false));
        assert!(ThemeMode::System.is_dark(true));
        assert!(!ThemeMode::System.is_dark(false));
    }

    #[test]
    fn 主题标签与顺序() {
        assert_eq!(ThemeMode::all().map(|m| m.label()), ["浅色", "深色", "跟随系统"]);
    }

    #[test]
    fn 尺寸常量来自原型() {
        assert_eq!(WINDOW_W, 1160.0);
        assert_eq!(WINDOW_H, 800.0);
        assert_eq!(TITLEBAR_H, 36.0);
        assert_eq!(WINDOW_BTN_W, 46.0);
        assert_eq!(SIDEBAR_W, 264.0);
        assert_eq!(STATUSBAR_H, 38.0);
    }
}
