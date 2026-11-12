//! 托盘可用性验证
//!
//! 只回答一个问题：不引入 winit/wry 的前提下，`tray-icon` 能不能在独立线程上
//! 建出托盘图标 + 原生菜单，并让事件回到我们的代码里。
//!
//! 这决定了「关窗到托盘」这条交互在 GPUI 单进程架构下成不成立。

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

/// 画一个 32×32 的占位图标：蓝底圆角 + 白色对勾，避免用外部资源文件
fn make_icon() -> Icon {
    const S: usize = 32;
    let mut buf = [0u8; S * S * 4];
    let r: f32 = 7.0; // 圆角半径

    for y in 0..S {
        for x in 0..S {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            // 圆角矩形遮罩
            let cx = fx.clamp(r, S as f32 - r);
            let cy = fy.clamp(r, S as f32 - r);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            if d > r {
                continue;
            }
            let i = (y * S + x) * 4;

            // 对勾：两条线段的距离场
            let on_check = {
                let seg = |ax: f32, ay: f32, bx: f32, by: f32| {
                    let (px, py) = (fx - ax, fy - ay);
                    let (vx, vy) = (bx - ax, by - ay);
                    let t = ((px * vx + py * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
                    ((px - t * vx).powi(2) + (py - t * vy).powi(2)).sqrt()
                };
                seg(9.0, 17.0, 14.0, 22.0) < 2.0 || seg(14.0, 22.0, 23.0, 11.0) < 2.0
            };

            if on_check {
                buf[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else {
                buf[i..i + 4].copy_from_slice(&[0x25, 0x63, 0xeb, 255]);
            }
        }
    }
    Icon::from_rgba(buf.to_vec(), S as u32, S as u32).expect("图标数据非法")
}

fn main() {
    println!("spike-tray pid={}", std::process::id());

    let menu = Menu::new();
    let open = MenuItem::new("打开主窗口", true, None);
    let run_now = MenuItem::new("立即签到全部", true, None);
    let pause = MenuItem::new("暂停全部任务", true, None);
    let quit = MenuItem::new("退出", true, None);

    menu.append_items(&[
        &open,
        &PredefinedMenuItem::separator(),
        &run_now,
        &pause,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .expect("菜单构建失败");

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("班级魔方自动签到 · 后台运行中")
        .with_icon(make_icon())
        .build()
        .expect("托盘创建失败");

    println!("托盘已创建，等待菜单事件（Ctrl+C 退出）");

    // 菜单事件在同一个线程的窗口消息循环里产生，这里用独立线程消费，
    // 证明「托盘线程只负责消息泵、业务逻辑在别处」这条分工是可行的。
    let open_id = open.id().clone();
    let quit_id = quit.id().clone();
    std::thread::spawn(move || {
        let rx = MenuEvent::receiver();
        loop {
            if let Ok(ev) = rx.recv() {
                if ev.id == open_id {
                    println!("[事件] 打开主窗口");
                } else if ev.id == quit_id {
                    println!("[事件] 退出");
                    std::process::exit(0);
                } else {
                    println!("[事件] 其他菜单项 {}", ev.id.0);
                }
            }
        }
    });

    // 托盘图标依赖创建它的线程上有一个 win32 消息循环，缺了它菜单不会弹。
    unsafe {
        let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = std::mem::zeroed();
        while windows_sys::Win32::UI::WindowsAndMessaging::GetMessageW(
            &mut msg,
            std::ptr::null_mut(),
            0,
            0,
        ) > 0
        {
            windows_sys::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            windows_sys::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }
    }
}
