//! `bjmf-rs`：命令行对拍工具。
//!
//! 存在的唯一目的是**在双跑期证明 Rust 版和 Python 版行为一致**：
//! 同一份 `data.json`、同一天，两边的状态序列应该逐行相同。
//!
//! ```text
//! bjmf-rs --once                 跑一轮签到（真实请求），退出码同 Python 版
//! bjmf-rs --check                只读：打印账号、时间点、下次执行时间
//! bjmf-rs --plan                 只读：打印下一次执行的时间（给脚本用）
//! ```
//!
//! 退出码：0 = 至少一个账号不是硬失败（或无账号）；1 = 全部硬失败；2 = 参数错误。

use std::path::PathBuf;
use std::process::ExitCode;

use bjmf_core::paths::Paths;
use bjmf_core::timepoints::{next_fire, parse_time_list};
use bjmf_core::{Config, Notifier, Runner, Settings, Site};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code as u8),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<i32, String> {
    let mut mode = Mode::Check;
    let mut config_override: Option<PathBuf> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--once" => mode = Mode::Once,
            "--check" => mode = Mode::Check,
            "--plan" => mode = Mode::Plan,
            "--config" | "-c" => {
                index += 1;
                let value = args.get(index).ok_or("--config 需要一个路径")?;
                config_override = Some(PathBuf::from(value));
            }
            "--version" | "-V" => {
                println!("bjmf-rs {VERSION}");
                return Ok(0);
            }
            "--help" | "-h" => {
                print_help();
                return Ok(0);
            }
            other => return Err(format!("未知参数：{other}\n\n{}", help_text())),
        }
        index += 1;
    }

    let paths = Paths::for_current_process();
    let _ = paths.ensure();
    let config_path = config_override.unwrap_or_else(|| paths.config_file());
    let settings = Settings::load(paths.settings_file());

    match mode {
        Mode::Check => {
            print_check(&config_path, &settings);
            Ok(0)
        }
        Mode::Plan => {
            let times = parse_time_list(&settings.times).map_err(|e| e.to_string())?;
            let now = chrono::Local::now().naive_local();
            let next = next_fire(&times, now).map_err(|e| e.to_string())?;
            println!("{}", next.format("%Y-%m-%d %H:%M:%S"));
            Ok(0)
        }
        Mode::Once => {
            let site = Site::with_default_base().map_err(|e| e.to_string())?;
            let notifier = Notifier::production().map_err(|e| e.to_string())?;
            let mut runner = Runner::new(&site, &notifier, &config_path, |line| println!("{line}"));
            let report = runner.run_once();
            for (name, status) in report.statuses() {
                println!("RESULT\t{name}\t{status}");
            }
            Ok(report.exit_code())
        }
    }
}

enum Mode {
    Check,
    Plan,
    Once,
}

fn print_check(config_path: &std::path::Path, settings: &Settings) {
    println!("数据文件：{}", config_path.display());
    let config = Config::load(config_path).unwrap_or_default();
    println!("账号数：{}", config.students.len());
    for (index, student) in config.students.iter().enumerate() {
        println!(
            "  [{}] {:<10} 班级 {:<12} 坐标 {},{}\t通知 {}",
            index + 1,
            if student.name.trim().is_empty() {
                "未命名"
            } else {
                student.name.as_str()
            },
            if student.class.trim().is_empty() {
                "未填写"
            } else {
                student.class.as_str()
            },
            if student.lat.trim().is_empty() { "-" } else { &student.lat },
            if student.lng.trim().is_empty() { "-" } else { &student.lng },
            if student.wx_key.trim().is_empty() && student.qmsg_key.trim().is_empty() {
                "未配置"
            } else {
                "已配置"
            },
        );
    }
    println!(
        "执行时间：{}",
        settings.times.join(", ")
    );
    println!(
        "失败重试：{} 次，间隔 {} 分钟",
        settings.retry_attempts, settings.retry_delay_minutes
    );

    if let Ok(times) = parse_time_list(&settings.times) {
        let now = chrono::Local::now().naive_local();
        if let Ok(next) = next_fire(&times, now) {
            let seconds = (next - now).num_seconds().max(0);
            println!(
                "下次执行：{}（{}）",
                next.format("%Y-%m-%d %H:%M:%S"),
                bjmf_core::humanize_delta(seconds)
            );
        }
    }
}

fn help_text() -> &'static str {
    "用法：bjmf-rs [--once | --check | --plan] [--config <data.json>]"
}

fn print_help() {
    println!("{}", help_text());
    println!();
    println!("  --once      跑一轮签到，退出码与 Python 版一致");
    println!("  --check     打印账号、时间点与下次执行时间（只读）");
    println!("  --plan      只打印下一次执行时间（YYYY-MM-DD HH:MM:SS）");
    println!("  --config    指定 data.json；默认用数据目录里的那份");
    println!("  --version   版本号");
}
