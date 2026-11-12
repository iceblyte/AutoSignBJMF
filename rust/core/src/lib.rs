//! 班级魔方自动签到 · 核心库
//!
//! 目标是与 `utils/` 下的 Python 实现**逐位对齐**（同一份 `data.json`、同样的
//! 时间点解析与下一次触发计算），这样 `bjmf-rs --once` 才能直接与 `BJMF.py`
//! 对拍。这里只放无 UI、无网络副作用的部分。

pub mod model;
pub mod notify;
pub mod paths;
pub mod qr;
pub mod runner;
pub mod settings;
pub mod scheduler;
pub mod site;
pub mod timepoints;

pub use model::{Config, Student};
pub use notify::{Message, Notifier};
pub use paths::{home_dir, Paths, APP_NAME};
pub use qr::QrLogin;
pub use runner::{Report, Runner};
pub use settings::Settings;
pub use scheduler::{Event, Hooks, Scheduler};
pub use site::{Profile, SignOutcome, SignStatus, Site, SiteError};
pub use timepoints::{humanize_delta, next_fire, parse_times, Clock, TimeError};
