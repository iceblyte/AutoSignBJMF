//! 数据目录解析。
//!
//! 规则与 Python 的 `desktop/paths.py` 完全一致：
//!
//! - **便携模式**：exe 同级存在 `data/` 目录 → 就地读写，U 盘可带走
//! - **常规模式**：没有 `data/` → `%USERPROFILE%\AutoSignBJMF\`
//!
//! 判定只看 exe 所在目录里有没有 `data/`，与当前工作目录无关
//! （双击、快捷方式、命令行在任何 cwd 启动，行为都一样）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 用户目录下的数据根名，也是注册表里 Run 项的值名。
pub const APP_NAME: &str = "AutoSignBJMF";

/// 已解析的数据目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    root: PathBuf,
    portable: bool,
}

impl Paths {
    /// 纯函数版：给定 exe 目录与用户主目录，算出数据目录。
    ///
    /// 抽成纯函数是为了能测：调用方在测试里传临时目录，不碰真实用户目录。
    pub fn resolve(exe_dir: &Path, home: &Path) -> Self {
        let portable_dir = exe_dir.join("data");
        if portable_dir.is_dir() {
            Self {
                root: portable_dir,
                portable: true,
            }
        } else {
            Self {
                root: home.join(APP_NAME),
                portable: false,
            }
        }
    }

    /// 按当前进程的真实路径解析。
    ///
    /// 环境变量 `AUTOSIGN_DATA_DIR` 可以强制指定数据目录：便携部署、做演示、
    /// 或者在同一台机器上跑两套互不干扰的数据时用得上。
    pub fn for_current_process() -> Self {
        if let Some(dir) = std::env::var_os("AUTOSIGN_DATA_DIR") {
            if !dir.is_empty() {
                return Self {
                    root: PathBuf::from(dir),
                    portable: false,
                };
            }
        }
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        Self::resolve(&exe_dir, &home_dir())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_portable(&self) -> bool {
        self.portable
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("data.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// 确保数据目录与 `logs/` 存在。
    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(self.log_dir())
    }

    /// 把老版本散落在 exe 旁边的 `data.json` 搬进数据目录。
    ///
    /// 只在目标文件不存在时动手；失败不阻断启动（用户重新扫码即可）。
    pub fn migrate_legacy(&self, exe_dir: &Path) -> Option<PathBuf> {
        let destination = self.config_file();
        if destination.exists() {
            return None;
        }
        let candidates = [exe_dir.join("data.json"), exe_dir.join("data").join("data.json")];
        for candidate in candidates {
            if candidate.is_file() && candidate != destination {
                if fs::copy(&candidate, &destination).is_ok() {
                    return Some(candidate);
                }
            }
        }
        None
    }
}

/// `%USERPROFILE%`（Windows）或 `$HOME`（其它平台）。
pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("bjmf-core-paths").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn 有_data_目录时走便携模式() {
        let exe = sandbox("portable/exe");
        let home = sandbox("portable/home");
        fs::create_dir_all(exe.join("data")).unwrap();

        let paths = Paths::resolve(&exe, &home);
        assert!(paths.is_portable());
        assert_eq!(paths.root(), exe.join("data").as_path());
        assert_eq!(paths.config_file(), exe.join("data").join("data.json"));
    }

    #[test]
    fn 没有_data_目录时落到用户目录() {
        let exe = sandbox("user/exe");
        let home = sandbox("user/home");

        let paths = Paths::resolve(&exe, &home);
        assert!(!paths.is_portable());
        assert_eq!(paths.root(), home.join(APP_NAME).as_path());
        assert_eq!(paths.settings_file(), home.join(APP_NAME).join("settings.json"));
    }

    #[test]
    fn ensure_建出日志目录() {
        let exe = sandbox("ensure/exe");
        let home = sandbox("ensure/home");
        let paths = Paths::resolve(&exe, &home);

        paths.ensure().unwrap();
        assert!(paths.log_dir().is_dir());
    }

    #[test]
    fn 迁移老配置只在目标不存在时发生() {
        let exe = sandbox("migrate/exe");
        let home = sandbox("migrate/home");
        fs::write(exe.join("data.json"), r#"{"students":[]}"#).unwrap();
        // 老版本也可能是 data/data.json（便携目录里）
        fs::create_dir_all(exe.join("data")).unwrap();

        let paths = Paths::resolve(&exe, &home);
        // 便携模式下 data/ 就是根，直接命中，不需要迁移
        assert!(paths.is_portable());

        let user_paths = Paths {
            root: home.join(APP_NAME),
            portable: false,
        };
        user_paths.ensure().unwrap();
        let moved = user_paths.migrate_legacy(&exe);
        assert!(moved.is_some());
        assert!(user_paths.config_file().is_file());

        // 第二次调用不再覆盖
        assert!(user_paths.migrate_legacy(&exe).is_none());
    }
}
