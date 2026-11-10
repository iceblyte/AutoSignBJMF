//! Tauri 常驻内存 spike：
//! - 默认启动不创建窗口（模拟托盘常驻）
//! - `--open` 启动时创建窗口
//! - `--close-after N` 创建后第 N 秒关窗，用于测量关窗后是否释放 WebView2

use std::time::Duration;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
fn ping() -> String {
    "pong".into()
}

fn open_window(app: &tauri::AppHandle) {
    if app.get_webview_window("main").is_some() {
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("bjmf spike")
        .inner_size(980.0, 640.0)
        .build();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let open_now = args.iter().any(|a| a == "--open");
    let close_after: Option<u64> = args
        .iter()
        .position(|a| a == "--close-after")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok());

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ping])
        .setup(move |app| {
            if open_now {
                open_window(app.handle());
            }
            if let Some(secs) = close_after {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(secs));
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.close();
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build tauri app")
        .run(|_app, event| {
            // 没有窗口时不要退出：常驻场景下窗口只是按需打开
            if let RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
