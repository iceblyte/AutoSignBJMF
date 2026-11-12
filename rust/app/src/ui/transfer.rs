//! 配置导入 / 导出。
//!
//! 导出的是一个自包含 JSON：`{"version":1,"students":[...],"settings":{...}}`。
//! 不含日志、不含运行状态，也不含 WXKey 之外的任何缓存 —— 但它**仍然包含 Cookie**，
//! 所以界面上的文案必须提醒「请勿分享」。
//!
//! 解析放在这里做成纯函数，文件对话框只负责拿路径。

use bjmf_core::{Config, Settings};
use serde_json::{json, Value};

/// 当前导出格式版本。以后字段变了就升版本，导入端按版本兼容。
pub const BUNDLE_VERSION: u32 = 1;

/// 把账号与设置打包成一个 JSON 字符串。
pub fn export_bundle(config: &Config, settings: &Settings) -> String {
    let bundle = json!({
        "version": BUNDLE_VERSION,
        "exportedAt": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "students": config.students,
        "settings": {
            "times": settings.times,
            "retry_attempts": settings.retry_attempts,
            "retry_delay_minutes": settings.retry_delay_minutes,
            "autostart": settings.autostart,
            "minimize_to_tray": settings.minimize_to_tray,
            "acc": settings.acc,
        }
    });
    serde_json::to_string_pretty(&bundle).unwrap_or_default()
}

/// 解析一份导出文件。
///
/// 兼容两种输入：
/// - 本程序的导出格式（带 `version` / `students` / `settings`）
/// - **裸 `data.json`**（只有 `students`）——方便从 Python 版直接迁移过来
pub fn import_bundle(text: &str) -> Result<(Config, Settings), String> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("不是合法的 JSON：{error}"))?;

    if !value.is_object() {
        return Err("文件内容不是一个 JSON 对象".to_string());
    }

    let students_value = value.get("students").cloned().unwrap_or(Value::Null);
    if students_value.is_null() {
        return Err("文件里没有 students 字段，可能不是本程序的配置".to_string());
    }
    if !students_value.is_array() {
        return Err("students 应该是数组".to_string());
    }

    let config: Config =
        serde_json::from_value(json!({ "students": students_value })).map_err(|error| {
            format!("账号数据格式不正确：{error}")
        })?;

    let settings = match value.get("settings") {
        Some(settings) => Settings::normalize(settings),
        // 裸 data.json 没有 settings：保持默认，导入后用户自己调时间
        None => Settings::default(),
    };

    Ok((config, settings))
}

/// 导入前的摘要，用于界面确认。
pub fn describe(config: &Config) -> String {
    let count = config.students.len();
    let names: Vec<&str> = config
        .students
        .iter()
        .take(3)
        .map(|student| student.name.as_str())
        .collect();
    if names.is_empty() {
        return "文件里没有账号".to_string();
    }
    if count <= 3 {
        format!("{count} 个账号：{}", names.join("、"))
    } else {
        format!("{count} 个账号：{} 等", names.join("、"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bjmf_core::Student;

    fn sample() -> (Config, Settings) {
        let config = Config {
            students: vec![
                Student {
                    name: "张明远".into(),
                    class: "2000001".into(),
                    lat: "39.908720".into(),
                    lng: "116.397450".into(),
                    acc: "30".into(),
                    cookie: "remember_student_abc=token".into(),
                    wx_key: "SCT123".into(),
                    ..Default::default()
                },
                Student {
                    name: "林可欣".into(),
                    ..Default::default()
                },
            ],
        };
        let settings = Settings {
            times: vec!["07:30:00".into(), "21:31:00".into()],
            retry_attempts: 5,
            retry_delay_minutes: 10,
            ..Settings::default()
        };
        (config, settings)
    }

    #[test]
    fn 导出再导入内容不丢() {
        let (config, settings) = sample();
        let text = export_bundle(&config, &settings);
        let (loaded, loaded_settings) = import_bundle(&text).unwrap();

        assert_eq!(loaded.students.len(), 2);
        assert_eq!(loaded.students[0].name, "张明远");
        assert_eq!(loaded.students[0].cookie, "remember_student_abc=token");
        assert_eq!(loaded.students[0].wx_key, "SCT123");
        assert_eq!(loaded_settings.times, vec!["07:30:00", "21:31:00"]);
        assert_eq!(loaded_settings.retry_attempts, 5);
        assert_eq!(loaded_settings.retry_delay_minutes, 10);
    }

    #[test]
    fn 导出带版本号与时间() {
        let (config, settings) = sample();
        let text = export_bundle(&config, &settings);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["version"], 1);
        assert!(value["exportedAt"].as_str().unwrap().contains('-'));
    }

    #[test]
    fn 可以直接导入裸_data_json() {
        let raw = r#"{"students":[{"name":"郑鸿彬","class":"91508","lat":"26.04","lng":"119.19","cookie":"c"}]}"#;
        let (config, settings) = import_bundle(raw).unwrap();
        assert_eq!(config.students.len(), 1);
        assert_eq!(config.students[0].class, "91508");
        // 没有 settings → 用默认
        assert_eq!(settings.times, vec!["21:31:00"]);
    }

    #[test]
    fn 坏_json_给出可读错误() {
        let error = import_bundle("{not json").unwrap_err();
        assert!(error.contains("JSON"));
    }

    #[test]
    fn 缺_students_时报错() {
        assert!(import_bundle(r#"{"settings":{}}"#).unwrap_err().contains("students"));
        assert!(import_bundle("[]").unwrap_err().contains("JSON 对象"));
    }

    #[test]
    fn students_不是数组时报错() {
        assert!(import_bundle(r#"{"students":"oops"}"#)
            .unwrap_err()
            .contains("数组"));
    }

    #[test]
    fn 未知字段被忽略且不报错() {
        let raw = r#"{"students":[{"name":"A","futureField":1}],"extra":true}"#;
        let (config, _) = import_bundle(raw).unwrap();
        assert_eq!(config.students[0].name, "A");
    }

    #[test]
    fn 摘要最多列三个名字() {
        let (mut config, _) = sample();
        assert_eq!(describe(&config), "2 个账号：张明远、林可欣");

        for index in 0..5 {
            config.students.push(Student {
                name: format!("账号{index}"),
                ..Default::default()
            });
        }
        let text = describe(&config);
        assert!(text.starts_with("7 个账号"));
        assert!(text.ends_with("等"));
    }

    #[test]
    fn 空文件的摘要() {
        assert_eq!(describe(&Config::default()), "文件里没有账号");
    }
}
