# Tauri 内存 spike

验证「Rust + Tauri 2 + Vue 3」能否替代当前 Python 桌面端，重点是常驻内存。

## 结论（Windows 11，release 构建，进程树口径）

| 场景 | 主进程 | 进程树 | 进程数 |
| --- | --- | --- | --- |
| 常驻、不建窗口 | 13.1 MB | **23.9 MB** | 2 |
| 打开窗口 | 40.4 MB | 401 MB | 8 |
| 关窗后 7 秒 | 33.3 MB | 187.6 MB | 4 |
| 关窗后 20 秒 | 33.2 MB | 188.5 MB | 4 |

对比：Python + tkinter 常驻 81 MB；Python + WebView2 常驻 530 MB。

要点：

1. Rust 侧空转只要 24 MB，比 tkinter 省 70%，比 WebView 版省 95%。
2. **WebView2 关窗后进程池不会退出**，仍有 4 个进程约 188 MB，20 秒内不释放。
   所以「按需开窗」必须让 UI 跑在独立子进程里，关窗即退出该进程，才能回到 24 MB。
3. release 二进制约 3.2 MB（lto + strip），当前 PyInstaller onefile 是 20 MB。
4. 中文渲染正常（走系统字体），圆角/阴影/暗色媒体查询都可用。

## 结构

- `tauri-app/src/main.rs`：默认不建窗口；`--open` 建窗口；`--close-after N` 到点关窗；无窗口时不退出进程
- `tauri-app/ui/index.html`：最小页面，验证 Rust 命令调用（`ping`）
- `tauri-app/tauri.conf.json`：`app.windows: []`，前端目录 `ui`

## 复现

```bash
cd spike/tauri-app
cargo build --release
./target/release/bjmf-spike.exe              # 常驻
./target/release/bjmf-spike.exe --open       # 开窗
```

> 仅为内存与可行性验证，未包含签到逻辑。
