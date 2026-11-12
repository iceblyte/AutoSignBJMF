# Rust 侧说明（spike 阶段）

## 为什么需要 fxc.exe

GPUI 的 Windows 后端（`gpui-pre-windows`）在 **release** 构建时会用 `fxc.exe`（Microsoft Direct3D Shader Compiler）
把 `src/shaders.hlsl` 预编译成字节数组，再 `include!` 进二进制：

```rust
// gpui-pre-windows/build.rs
#[cfg(not(debug_assertions))]
compile_shaders();   // 只有 release 才编译 HLSL → 只有 release 才需要 fxc
```

由此推得两条：

1. **debug 构建完全不需要 SDK**：着色器在运行时用 `D3DCompile` 编译（依赖系统自带的 `d3dcompiler_47.dll`）。
   所以 `cargo build`（不加 `--release`）在裸机上就能跑。
2. **release 必须有一个能执行的 `fxc.exe`**，否则 build.rs 会 `panic!("Failed to find fxc.exe")`。

查找顺序（源码里写死）：`GPUI_FXC_PATH` 环境变量 → `PATH` 里的 `fxc.exe` → 注册表里最新的 Windows SDK。

## 不装 SDK 拿到 fxc（本机已验证）

目标：只取一个 169 KB 的 `fxc.exe`，全程落 D 盘，不安装、不需要管理员权限。

```bash
# 1) 下官方 SDK 引导程序（1.4 MB）
curl -sL -o winsdksetup.exe "https://go.microsoft.com/fwlink/?linkid=2196241"

# 2) 只下「桌面 C++ x64」这一组组件到本地目录（约 440 MB，先下载不安装）
#    注意：Git Bash 会把 /layout 当成路径改写，必须关掉路径转换
MSYS_NO_PATHCONV=1 ./winsdksetup.exe /layout "D:\\sdk-layout" \
    /features OptionId.DesktopCPPx64 /quiet /norestart

# 3) fxc.exe 在 "Windows SDK for Windows Store Apps Tools-x86_en-us.msi" 里
#    但 cab 内文件名是哈希（filXXXX），需要读 MSI 的 File / Media 表才能还原
pip install pymsi
python rust/_tools/find_fxc.py "D:\\sdk-layout\\Installers" "D:\\Workplace\\...\\rust\\_tools\\fxc"

# 4) 指给 cargo
export GPUI_FXC_PATH="D:\\Workplace\\...\\rust\\_tools\\fxc\\fxc.exe"
cargo build --release
```

`find_fxc.py` 做三件事：遍历所有 MSI 的 `File` 表找 `fxc.exe` → 用 `Media` 表定位所属 cab 与内部键名 →
用 7z 取出并按 PE 头判断架构，只保留 x64 那份。三个架构的 fxc 分别是 x86(138 KB)/arm64(155 KB)/x64(169 KB)。

**不要提交 `fxc.exe`**：它是微软的 SDK 组件，`/rust/_tools/` 已在 `.gitignore` 里。
CI 上的做法是装 Windows SDK（`choco install windows-sdk-10.0` 或 msbuild 环境），不要让 CI 走这条野路子。

## 当前实测数据（Windows，单进程，release + 全 LTO）

| 指标 | 结果 |
|---|---|
| 进程数 | **1** |
| 开窗内存 | Working Set **54.5 MB** / Private **100 MB** |
| 隐藏窗口后（等价关窗到托盘） | Working Set **54.5 MB**（不回落，也不再增长） |
| 启动到窗口可见 | **486 ms**（3 次中位数） |
| release 体积 | **10.27 MB**（`lto=true, codegen-units=1, opt-level=s, strip`） |
| release 构建耗时 | **8 分 22 秒**（全量）；关 LTO 为 3 分 48 秒 / 34.5 MB |
| debug 构建耗时 | 首次全量约 25 秒（增量 5~10 秒） |
| 运行时依赖 | **全部是系统 DLL**（kernel32/d3d11/dwrite/dcomp/icuuc…），无 VC++ 运行库、无 MinGW 动态库，单文件即可分发 |

对照 Tauri 方案（你们之前的 spike）：开窗 401 MB / 8 进程、关窗残留 188 MB、安装包 3.2 MB。

> 早前那次「LTO 跑了 50 分钟没出结果」是残留 `rustc` 进程挂死（CPU 0.45 秒），不是 LTO 本身慢。
> 重跑后 8 分 22 秒完成，这个数字才是可引用的。

## 托盘：已验证可行

`tray-icon 0.26` 在独立线程上创建图标 + 原生菜单，线程里跑一个裸 win32 `GetMessageW` 消息循环，
菜单事件通过 `MenuEvent::receiver()` 交给业务线程消费——不需要 winit，也不需要 GPUI 参与。
实测结果：

- 图标正常显示（32×32 RGBA 自绘：蓝底圆角 + 白色对勾），见 `rust/shots/gpui-tray-icon.png`
- Shell 已登记到 `HKCU\Control Panel\NotifyIconSettings`，可按 exe 路径定位
- **Win11 默认把新托盘图标折叠进「隐藏的图标」**，需要把该项的 `IsPromoted` 设为 1 才常驻；
  产品里应在首次运行时写这个值，或在设置页给「在任务栏常驻显示」的引导

验证程序在 `rust/spike-tray/`（`cargo run` 后看通知区域）。

## 结论

GPUI 单进程就能同时满足「开窗 55 MB」「关窗后不掉到几百兆」「托盘常驻」「10 MB 单文件」四条，
因此**不需要**之前设想的「守护进程 + 按需 UI」双进程方案，也不必再走 Tauri。

代价（需要在正式开发前认下来）：

1. **还原度约 95%，不是 100%**：设计稿里 10.5–11.5px 的辅助文字在 GPUI 上要提到 12–13px，密度会略松。
2. **没有现成组件库**：`gpui-kit` 给的是基础件，表格、抽屉、状态 pill 仍要自己写（工作量真实存在）。
3. **gpui 本体 pre-1.0**，靠 `gpui-kit` 锁定版本，升级要跟着它走。
4. release 构建必须能拿到 `fxc.exe`（见上一节），CI 要装 SDK。

## 两个 spike 目录

| 目录 | 用途 | 状态 |
|---|---|---|
| `rust/spike-gpui` | GPUI + gpui-kit，按 `design/v2/10-exe.html` 搭的主界面缩影 | **可运行**，是本轮结论的依据 |
| `rust/spike-egui` | egui 0.29 对照实现 | 渲染质量明显不及，内存数据不可信（走的是软件光栅），保留作对照 |
| `rust/spike-tray` | 托盘 + 原生菜单最小验证 | **已验证**，见 `rust/shots/gpui-tray-icon.png` |

`rust/_tools/probe.py` 是截图/量内存的小脚本（`shot` / `mem` / `hide` 三个动作），不参与产品。

## 已知问题

- 自绘标题栏需要 `WindowOptions.titlebar.appears_transparent = true`，否则会出现「系统标题栏 + 自绘标题栏」两条。
- 窗口按钮用 `window_control_area(WindowControlArea::Drag/Min/Max/Close)`，交给系统处理，不要自己写拖拽逻辑。
- 默认工具链已是 `stable-x86_64-pc-windows-msvc`（配 LLVM + xwin，见下文「MSVC 工具链」）。GNU 版仍可用 `cargo +stable-x86_64-pc-windows-gnu` 构建。

---

## 重构进度（Rust 版）

### 已完成

| 模块 | 内容 | 测试 |
|---|---|---|
| `core::model` | `data.json` 读写，字段大小写与未知字段保留 | 5 |
| `core::timepoints` | 时间点解析、下一次触发、中文时长 | 13 |
| `core::settings` | `settings.json` 收敛（1–10 次 / 1–60 分钟）、原子写 | 11 |
| `core::paths` | 便携/用户目录判定、老配置迁移、`AUTOSIGN_DATA_DIR` 覆盖 | 4 |
| `core::site` | 签到主流程（列表探测 → 提交 → 状态判定）+ HTML 解析 | 29 |
| `core::notify` | Server 酱 / Qmsg 酱，汇总文案 | 12 |
| `core::qr` | 扫码登录：取码 → 轮询 → 换 Cookie | 7 |
| `core::runner` | 一轮编排：补班级、写回、汇总通知、退出码 | 5 |
| `core::scheduler` | 到点执行、失败重试、暂停跳过（假时钟单测） | 11 |
| `app::theme` | 原型令牌（明/暗）、系统主题读取、尺寸常量 | 6 |
| `app::state` | 编辑草稿、校验、四态判定、文案格式、日志环形缓冲 | 17 |
| `app::ui` | 主窗口：标题栏 / 侧栏 / 三面板 / 状态栏 / 空态；自定义输入框 | 3 |
| `app::system` | 单实例互斥、开机自启、托盘菜单（含图标位图） | 6 |
| `app::runtime` | 调度线程 + 托盘线程 + 共享快照 | — |
| `cli` | `bjmf-rs --once / --check / --plan` 对拍工具 | — |

合计 **132 个测试**（`cargo test --workspace`）。

### 已完成（第二轮）

- 屏 03「添加账号 · 扫码」：模态 + 后台线程拉码/轮询/换 Cookie，成功后直接写入 `data.json` 并刷新侧栏
- 屏 04/05「设置」：860×560 覆盖层，四项导航（常规 / 通知 / 数据 / 关于），主题三段式可切、关窗行为可切
- 关窗拦截：`on_window_should_close` 里拦下关闭并弹一次性说明，调度不会因为关窗而停
- 运行期图片：`assets::put_image()` 让二维码这类运行时图片也能走 `img()`

### 已完成（第三轮）

- **真正的关窗到托盘**：`system::win` 按进程 id 找 HWND，关窗时 `ShowWindow(SW_HIDE)`，
  进程与调度继续跑；托盘「打开主窗口」走 `ShowWindowAsync + SetForegroundWindow` 恢复（最大化状态保留）。
  回归脚本：`python rust/_tools/close_test.py`（拉起 → 发 WM_CLOSE → 断言隐藏且进程存活）
- 打包：`build/build-rust.ps1`（`-Portable` 额外出一份便携 zip，包内不含任何账号数据）

### 已完成（第五轮）

- **导入 / 导出配置**：`ui/transfer.rs` 负责打包与解析（9 个单测），文件对话框用 `rfd`。
  导出格式带 `version`/`exportedAt`；导入兼容**裸 `data.json`**，方便从 Python 版直接迁移
- **托盘文案实时刷新**：每 2 秒重读一次开机自启与暂停状态，改设置后菜单文字立刻跟上
- **外部命令不再弹黑窗**：统一走 `hidden_command()`（`CREATE_NO_WINDOW`）
- 不可用的按钮全部置灰（单实例开关标注「始终开启」），不再出现「看着能点其实没反应」

### MSVC 工具链：已落地，且没有安装 Visual Studio

用 LLVM 的工具链 + 官方 VS/SDK 的 CRT 与库拼出一套 MSVC 环境，全部落在 D 盘，不需要管理员、不写 C 盘：

| 组成 | 来源 | 位置 |
|---|---|---|
| `clang-cl` / `llvm-ml` / `llvm-lib` | `scoop install llvm`（23.1.5） | `D:\Apps\Scoop\User\apps\llvm\current\bin` |
| `lld-link`（MSVC 兼容链接器） | 同上 | 同上 |
| MSVC CRT（`msvcrt.lib` / `libcmt.lib` / 头文件） | VS 2022 17.14 的包，由 `cargo-xwin` 解出 | `D:\Tools\xwin-cache\xwin\crt` |
| Windows SDK 10.0.26100（`kernel32.lib` / `um` / `ucrt`） | 同上 | `D:\Tools\xwin-cache\xwin\sdk` |

复现步骤：

```bash
scoop install llvm                        # 装完重开终端，让 PATH 生效
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
export XWIN_CACHE_DIR='D:\Tools\xwin-cache'
cargo xwin cache xwin                     # 拉 CRT + SDK，约 2.4 GB
```

然后 `rust\.cargo\config.toml` 把 MSVC 目标指向 `lld-link`，并用 `INCLUDE` / `LIB` 指到上面两处（已写好，换机器只改路径）。
需要编译 C 的 crate 通过 `CC_x86_64_pc_windows_msvc=clang-cl`、`AR_...=llvm-lib` 走 clang-cl；`ring` 的汇编由 `llvm-ml` 汇编。

**实测（MSVC 目标，本机）：**

| 指标 | MSVC 版 | GNU 版（上一轮） |
|---|---|---|
| release 体积 | **10.41 MB**（动态 CRT）/ **10.56 MB**（静态 CRT） | 10.7 MB |
| 开窗内存 | **70.9 MB** WS / 100.4 MB Private | 59.4 MB WS / 89.4 MB Private |
| debug 全量构建 | 6 分 28 秒 | 约 25 秒（增量） |
| release 构建（LTO） | 8 分 27 秒 | 11 分 16 秒 |
| 测试 | 143 全绿 | 143 全绿 |
| 运行库依赖 | `VCRUNTIME140.dll` + UCRT（Win10+ 自带 UCRT） | 无（静态链接 mingw） |

`VCRUNTIME140.dll` 是 VC++ 2015–2022 运行库，绝大多数机器上已有；要彻底零依赖就用静态 CRT（`dist/AutoSignBJMF-rs.exe` 就是这一版）：

```bash
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release -p bjmf-app --target x86_64-pc-windows-msvc
```

静态 CRT 版实测 **10.56 MB**，导入表只剩 28 个系统 DLL，与 GNU 版一样可直接单文件分发（构建时会有 `libvcruntime.lib` 的 LNK4099 警告，是缺 pdb，不影响产物）。

回退到 GNU：`cargo +stable-x86_64-pc-windows-gnu build -p bjmf-app`（`config.toml` 只有 msvc 段，不影响 gnu）。

### 已完成（第四轮）

- **exe 图标与版本信息**：`app/app.rc` + `build.rs` 用 `embed-resource` 编入（GNU 走 windres，MSVC 走 rc.exe）。
  ico 由 `design/v2/assets/logo.png` 生成，7 档尺寸，四周留白 22%（32px 下轨道不会被裁）
- **托盘双击打开主窗口**：`TrayIconEvent::DoubleClick` → 置 `show_requested` → 下一帧 `ShowWindow`

### 仍未完成

- 托盘菜单的「开机自启」文案改完设置后要重启才刷新（注册表已即时生效）

### 本地运行

```bash
cd rust
cargo run -p bjmf-app                       # 用默认数据目录（%USERPROFILE%\AutoSignBJMF）
AUTOSIGN_DATA_DIR=app/demo cargo run -p bjmf-app   # 用演示数据（3 个账号）
AUTOSIGN_NO_TRAY=1 cargo run -p bjmf-app    # 不起托盘（截图/排查用）
cargo test --workspace                      # 全部测试
```
