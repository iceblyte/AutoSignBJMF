# 班级魔方多人GPS自动签到

- Thanks To [JasonYANG170/AutoCheckBJMF](https://github.com/JasonYANG170/AutoCheckBJMF) ，根据自己学校的签到进行了简化
- 仅根据自己学校的班级魔方需求更改简化代码,其他功能请到项目[AutoCheckBJMF](https://github.com/JasonYANG170/AutoCheckBJMF)项目查看其他内容
- 可配置多人签到
- 可配置QQ/WX通知签到情况
- 上游 [emocat17/AutoSignBJMF](https://github.com/emocat17/AutoSignBJMF) 已于 2026.09.01 停止维护，本仓库在此基础上继续维护
- 如果你觉得好用,`Please Star`orz
  ![网页端展示](doc/img0.jpg)

> 上游原作者的学校签到方式已变更，签到逻辑因此不再更新；本仓库保留了 GPS 签到这条仍然可用的路径，
> 并补齐了「到点一定能跑」的三种运行方式。

## 三种运行方式

| 方式 | 适合谁 | 特点 |
| --- | --- | --- |
| [桌面版 exe](#方式零桌面版-exe最省事) | 只想双击运行的人 | 图形界面选时间、选坐标，关窗口缩到托盘继续跑 |
| [GitHub Actions](#方式一github-actions免服务器) | 不想养机器的人 | 零成本，但官方文档明确说明 `schedule` 事件在高负载时段会被延迟，定时不保证准点 |
| [本地常驻](#方式二本地常驻到自己电脑上跑) | 白天电脑常开的学生 | 用系统本地时间触发，到点即跑，不受 Actions 排队影响 |
| [服务器部署](#方式三服务器部署docker-或-systemd) | 有 VPS /  NAS 的人 | 24 小时在线，最稳定 |

四种方式最终执行的都是同一套签到逻辑（`utils/runner.py` → `utils/attendance.py`），只是触发器不同。

## 方式零：桌面版 exe（最省事）

双击 `AutoSignBJMF.exe` 打开窗口：

1. 「扫码添加 / 更新」用微信扫码，自动填入 Cookie、姓名、班级（支持多个账号）
2. 「打开高德坐标拾取器」→ 浏览器打开 <https://lbs.amap.com/tools/picker>，选好位置复制坐标，粘回弹窗
3. 「每天执行时间」里加多个时间点（如 07:30、21:31），可配失败重试次数与间隔
4. 点「启动后台运行」，关掉窗口就缩到右下角托盘，签到继续

数据放在哪：

| 形态 | 数据位置 |
| --- | --- |
| 绿色版（exe 同级有 `data/`） | 就近写 `data/`，整个目录可以拷到 U 盘 |
| 单文件版（只有 exe） | `C:\Users\<用户名>\AutoSignBJMF\` |

里面是 `data.json`（账号/Cookie/坐标）、`settings.json`（时间点）、`logs/`（运行日志）。

源码直接运行（开发用）：

```bash
uv pip install -r requirements-desktop.txt
python desktop/main.py
```

打包（需要 `pyinstaller`）：

```powershell
uv pip install -r requirements-desktop.txt pyinstaller
powershell -ExecutionPolicy Bypass -File build\build.ps1

# 只出 exe，不压缩
powershell -ExecutionPolicy Bypass -File build\build.ps1 -SkipZip
```

产物：

- `dist\AutoSignBJMF.exe` —— 单文件版
- `dist\AutoSignBJMF\` —— 绿色版目录（含 `data\`）
- `dist\AutoSignBJMF-便携版.zip` —— 干净的压缩包，**不含任何 Cookie 配置**，可直接分发

打包后自检：

```powershell
.\dist\AutoSignBJMF.exe --self-check   # 检查依赖与数据目录，结果写到 selfcheck.txt
.\dist\AutoSignBJMF.exe --smoke        # 建一次隐藏窗口再销毁，验证 tkinter/托盘是否齐全
```

## 代码结构

```
AutoSignBJMF/
├── BJMF.py                     # 命令行入口（薄壳，逻辑在 utils/runner.py）
├── signin_loop.py              # 本地/服务器常驻脚本，按时间点每天定时执行 BJMF.py
├── auto_add_user.py            # 自动添加用户工具，通过微信扫码获取用户信息并写入 data.json
├── desktop/                    # 桌面版（tkinter + 托盘 + 内置调度）
│   ├── main.py                 # 入口，含 --self-check / --smoke 自检
│   ├── app.py                  # 主窗口
│   ├── qr_dialog.py            # 扫码登录窗口
│   ├── qr_client.py            # 二维码获取 / 轮询 / 换 Cookie
│   ├── scheduler.py            # 后台调度（失败重试、暂停恢复）
│   ├── settings.py             # settings.json 读写
│   ├── paths.py                # 绿色版 / 用户目录的路径判定
│   ├── tray.py                 # 系统托盘
│   └── autostart.py            # 开机自启（HKCU Run）
├── build/                      # PyInstaller 打包脚本与图标生成
├── requirements.txt            # 命令行版依赖
├── requirements-desktop.txt    # 桌面版依赖（额外 pystray）
├── requirements-dev.txt        # 开发/测试依赖
├── .env                        # (可选) 环境变量配置文件，用于配置公共参数
├── .github/workflows/
│   └── auto-signin.yml         # GitHub Actions 定时签到
├── deploy/
│   ├── register-windows-task.ps1   # Windows 计划任务注册脚本
│   └── signin-loop.service         # Linux systemd 常驻单元
├── tests/                      # pytest 单元测试（全程 mock，不会触发真实签到）
├── utils/                      # 工具模块目录
│   ├── __init__.py             # 模块初始化文件
│   ├── runner.py               # 单次签到流程（命令行与桌面端共用）
│   ├── timepoints.py           # 时间点解析与下次触发时间
│   ├── config_manager.py       # 配置文件管理模块
│   ├── user_info.py            # 用户信息获取模块
│   ├── notification.py         # 通知发送模块
│   └── attendance.py           # 签到任务执行模块
└── web_signin/                 # Flask + APScheduler 的 Web 管理端
```

### 各模块职责

- **BJMF.py**: 命令行入口，读取 `data.json` 并调用 `utils.runner.run_once`
- **desktop/**: 桌面版，签到时同样调用 `run_once`，只是触发器换成了界面里的时间点
- **signin_loop.py**: 常驻调度，睡眠到设定时间点再调用 `BJMF.py`
- **auto_add_user.py**: 自动添加用户工具，通过微信扫码自动获取 Cookie 和班级信息并写入 `data.json`；支持读取 `.env` 公共配置
- **config_manager.py**: 处理配置文件的读取和保存
- **user_info.py**: 获取用户信息和班级信息
- **notification.py**: 处理QQ和微信消息发送
- **attendance.py**: 执行签到任务的核心逻辑

## 功能

- 自动从指定课程中获取签到项（同时探测 `punchs` 与 `daka` 两类模块）
- 通过模拟表单提交，实现自动签到
- 签到成功后,发送QQ/WX消息通知(可选配,可以不用配置)
- 支持通过微信扫码快速添加用户
- 支持 `.env` 配置公共参数，简化多人配置

## 更新说明

- 2026.11.09
  - 新增桌面版 `desktop/`：tkinter 窗口 + 系统托盘，多账号管理、内置微信扫码登录、
    一键打开高德坐标拾取器、每天多个时间点、失败自动重试、关窗缩到托盘继续跑
  - 抽出 `utils/runner.py`（单次签到流程）与 `utils/timepoints.py`（时间点计算），
    命令行、常驻脚本、桌面端、Web 端共用同一套逻辑；`BJMF.py` 变为薄壳
  - 新增 `build/build.ps1`：PyInstaller 一键打包，产出单文件 exe、绿色版目录与
    便携版 zip（zip 内不含任何 Cookie 配置，可安全分发）
  - 数据目录双模式：exe 同级有 `data/` 时走绿色版，否则写到 `%USERPROFILE%\AutoSignBJMF`
  - 新增 `--self-check` / `--smoke` 两个自检开关，方便验证打包产物
  - 测试增加到 181 个，覆盖 runner、时间点、桌面端配置/调度/扫码/窗口构建

- 2026.11.06
  - 修复: `SQLAlchemy==2.0.29` 在 Python 3.13 下导入即崩（`AssertionError: ...directly inherits TypingOnly...`），
    升级到 `2.0.36`；此前 3.13 用户不仅 Web 端起不来，`pytest` 也会在收集阶段直接报错
  - 修复: `/api/admin/*` 恢复管理员鉴权（`X-Admin-Token`），此前 `_require_admin` 恒放行，
    只要端口可达任何人都能删任务；`/manage` 增加 Key 初始化 / 登录遮罩
  - 修复: 管理端日志时间按北京时间展示（此前用 UTC，比实际早 8 小时）
  - 修复: `punchs`/`daka` 两个列表页都请求失败时引用未绑定的 `response`，真实网络原因被 `NameError` 掩盖
  - 修复: `auto_add_user.py` 的 `data.json` 改为脚本同级（此前跟随 CWD），并给打印输出里的 Cookie / 推送 Key 打码
  - 新增: `tests/test_web_api.py`（29 个用例）覆盖全部 Flask 接口与鉴权分支；`tests/test_auto_add_user.py`
  - 脱敏: `doc/img0.jpg` 的示例经纬度、`doc/img5.jpg` 的班级 ID 已打码
  - 忽略: `login_qrcode.png`、`web_signin/debug_qr.html` 加入 `.gitignore`

- 2026.11.05
  - 新增 `signin_loop.py`：本地/服务器常驻定时，不依赖终端保持打开
  - 新增 `deploy/`：Windows 计划任务注册脚本、Linux systemd 单元
  - 新增 `tests/`：124 个 pytest 用例覆盖签到后端，全程 mock 不发真实请求
  - 新增 GitHub Actions 自动签到（可用，但注意官方延迟说明）
  - 修复: 以 `python web_signin/run.py` 启动时，Web 版定时任务因项目根不在 `sys.path` 上而必然失败
  - 修复: 登录态 Cookie 名随学校变化，改为按 `remember_student_` 前缀提取；提取不到时整串发送，`PHPSESSID` 会话不再直接报错
  - 修复: 汇总通知缺少超时，可能拖挂定时任务
  - 合并上游 `emocat17/AutoSignBJMF` 的签到检测逻辑重构
  - 统一依赖：`requests` 版本冲突、移除未使用的 `DateTime` 与 `bs4` 占位包
  - 补充 MIT 许可证


- 2026.01.10
  - `auto_add_user.py` 优化: 支持通过 `.env` 文件配置公共参数(经纬度、通知Key等)，简化配置流程
  - `auto_add_user.py` 优化: 增加二维码自动清理机制，避免垃圾文件堆积及文件占用问题
  - `utils/attendance.py` 修复: 优化签到状态检测逻辑，增加对"已签到"状态的HTML解析，解决正则匹配失败导致的误报问题

- 2025.12.15
  - 更新 `utils/attendance.py` ,改用 requests.Session()防止获取签到项失败问题；同时增加了对 response.url 的检测

- 2025.12.04 v2版本
  - 新增 `auto_add_user.py` 工具，实现微信扫码自动获取用户信息并写入配置文件data.json
  - 简化了用户添加流程，无需手动获取Cookie和班级ID

## 环境准备

需要 **Python 3.10 及以上**（代码使用了 `str | None` 语法）。

> Python 3.13 用户注意：Web 管理端依赖的 SQLAlchemy 必须 **≥ 2.0.32**（2.0.31 起初步支持 3.13，
> 2.0.32 才完整支持）。仓库已钉 `SQLAlchemy==2.0.36`；若你手工降到 2.0.29，`import app` 会直接抛
> `AssertionError: ...directly inherits TypingOnly...`，Web 端和调度器测试都跑不起来。

```bash
git clone https://github.com/iceblyte/AutoSignBJMF.git
cd AutoSignBJMF

# 建议用虚拟环境
python -m venv .venv
# Windows
.venv\Scripts\activate
# Linux / macOS
source .venv/bin/activate

# 命令行版依赖
pip install -r requirements.txt
```

## 配置指南

### 方法一：自动添加用户 (推荐)

最简单的方法，无需手动抓包或查找 Cookie。

1. **(可选) 配置公共参数**：
   在项目根目录下创建 `.env` 文件（可从 `.env.example` 复制），填入经纬度等公共信息。
   ```properties
   # 是否启用公共配置 (True/False)
   ENABLE_COMMON_CONFIG=True

   # 公共配置参数
   COMMON_LAT=xxxxxx  # 纬度
   COMMON_LNG=xxxxxx  # 经度
   COMMON_ACC=30      # 精度
   COMMON_QMSG_KEY=   # Qmsg推送Key(可选)
   COMMON_WX_KEY=     # Server酱推送Key(可选)
   ```

2. **运行自动工具**：
   ```bash
   python auto_add_user.py
   ```

3. **扫码登录**：
   使用微信扫描弹出的二维码，程序会自动获取 Cookie 和班级信息并保存到 `data.json`。

### 方法二：手动配置 (高级)

如果你需要手动配置 `data.json`，可以按照以下步骤获取参数。

#### 1. data.json 格式

从 `data.json.example` 复制一份改名为 `data.json`：

```json
{
    "students": [
        {
            "name": "用户备注",
            "class": "123456",
            "lat": "30.123456",
            "lng": "120.123456",
            "acc": "30",
            "cookie": "从浏览器获取的Cookie字符串",
            "QmsgKEY": "",
            "WXKey": ""
        }
    ]
}
```

> `WXKey` / `QmsgKEY` 写在每个学生对象里；留空则该学生不发通知。

#### 2. 参数获取说明

- **Cookie 获取方法 (PC端浏览器)**：
  1. 电脑微信登录并打开签到页面：`http://g8n.cn/student/login?ref=%2Fstudent`
  2. 按 `F12` 打开开发者工具，切换到 **网络 (Network)** 标签。
  3. 刷新页面，在左侧请求列表中找到第一个请求（通常是数字或 student）。
  4. 点击该请求，在右侧 **标头 (Headers)** -> **请求标头 (Request Headers)** 中找到 `Cookie`。
  5. 复制 `Cookie:` 后面的所有内容（通常以 `PHPSESSID=...` 或 `remember_student_...` 开头）。
  
  ![浏览器查看Cookie](doc/img5.jpg)

- **Class (班级ID)**：
  - 在上述浏览器页面的网址栏中，可以看到类似 `course/123456` 的内容，`123456` 即为班级ID。
  - 或者在页面中查看。
  ![浏览器查看班级码](doc/img4.jpg)

- **经纬度 (Lat/Lng)**：
  - 使用 [高德地图坐标拾取器](https://lbs.amap.com/tools/picker) 获取。

- **推送 Key (可选)**：
  - **Qmsg酱**: [官网](https://qmsg.zendee.cn/) 注册获取 Key。
  - **Server酱**: [官网](https://sct.ftqq.com/) 扫码获取 Key。

## 手动执行一次签到

```bash
python BJMF.py
```

无论用哪种自动方式，都建议先手动跑一次确认配置无误。

---

## 方式一：GitHub Actions（免服务器）

仓库已内置 [`.github/workflows/auto-signin.yml`](.github/workflows/auto-signin.yml)，每天定时执行一次签到，失败后每隔 10 分钟重试，最多 3 次。

### 配置步骤

1. **把 `data.json` 的内容存成 Secret**
   仓库页面 → `Settings` → `Secrets and variables` → `Actions` → `New repository secret`
   - Name: `DATA_JSON`
   - Secret: 粘贴 `data.json` 的**完整 JSON 文本**

2. **启用 workflow**
   仓库页面 → `Actions` → 顶部若出现 "This workflow is disabled" 提示 → `Enable workflows`。
   （仓库转公开、或长期没有运行记录时，GitHub 会把定时 workflow 置为 `disabled_manually` / `inactive`，必须手动启用，否则 cron 不会跑。）

3. **验证**
   `Actions` → `自动签到` → `Run workflow` 手动触发一次，看日志里是否签到成功。

4. **Cookie 过期后更新 Secret**
   登录态过期时日志里的用户姓名会显示 `未找到`，签到会直接跳过。重新扫码生成新的 `data.json` 后覆盖 Secret 即可：

   ```bash
   python auto_add_user.py                                    # 重新扫码，写入 data.json
   gh secret set DATA_JSON --repo <你的用户名>/<仓库名> < data.json
   ```

### 关于定时时间

```yaml
schedule:
  - cron: '31 13 * * *'   # UTC 13:31 = 北京时间 21:31
```

cron 用的是 **UTC**，不是本地时间。北京时间换算成 UTC 要减 8 小时。

> ⚠️ **定时不保证准点。** GitHub 官方文档明确说明 `schedule` 事件在执行高峰期会被延迟，
> 且最短间隔为 5 分钟、同一账户的定时任务过多时还可能被丢弃。
> 实测延迟可能从几分钟到几小时——如果你学校的签到窗口很短，请改用下面两种常驻方式。

## 方式二：本地常驻（自己电脑上跑）

`signin_loop.py` 是一个纯标准库的常驻进程：按**系统本地时间**睡眠到点，再调用 `BJMF.py`，跑完继续等下一个点。

```bash
# 先验证一次配置（立即执行一次后退出）
python signin_loop.py --once

# 常驻：每天 07:30 和 21:31 执行
python signin_loop.py --times 07:30,21:31
```

关掉终端进程就没了，所以配一个登录自启：

```powershell
# 需要管理员权限，注册一次即可（会隐藏控制台窗口）
.\deploy\register-windows-task.ps1 -Times 07:30,21:31

# 只想用 Web 管理端常驻
.\deploy\register-windows-task.ps1 -Mode web

# 先看看会注册什么，不改系统
.\deploy\register-windows-task.ps1 -DryRun

# 卸载
.\deploy\register-windows-task.ps1 -Uninstall
```

查看与手动触发：

```powershell
Get-ScheduledTask -TaskName BJMF-AutoSignIn
Start-ScheduledTask -TaskName BJMF-AutoSignIn
```

## 方式三：服务器部署（Docker 或 systemd）

### Docker（Web 管理端 + 内置调度）

`web_signin/` 自带 APScheduler，进程常驻即可每天按点执行，任务配置存在 SQLite 里。

```bash
# 用 compose 起（推荐）
docker compose up -d

# 或手动构建
docker build -t bjmf-web -f web_signin/Dockerfile .
docker run -d --name bjmf-web -p 9988:9988 --restart unless-stopped bjmf-web
```

访问 `http://服务器IP:9988` 扫码配置任务，时间点精确到秒，按 `Asia/Shanghai` 解释。

> 注意：容器重建会丢失数据库，请把 `./web_signin/db` 留在宿主机（compose 已挂载）。
> 安全提示：用户侧接口（`/api/qr`、`/api/tasks`、`/api/user/config`）**不做鉴权**，能访问端口的人即可提交和读取含 Cookie 的任务；
> `/api/admin/*` 需要管理员 Key（首次访问 `/manage` 会引导你设置，数据库只存 SHA-256 哈希）。
> 因此本地运行时默认只监听 `127.0.0.1`，需要对外服务时再显式设置 `WEB_SIGNIN_HOST=0.0.0.0`
>（Docker 镜像内已设置），并自行加一层反向代理鉴权或防火墙限制。


### systemd（命令行常驻）

```bash
sudo cp deploy/signin-loop.service /etc/systemd/system/
# 按实际路径修改 WorkingDirectory / ExecStart
sudo systemctl daemon-reload
sudo systemctl enable --now signin-loop

# 看日志
journalctl -u signin-loop -f
```

---

## web_signin Web 管理端

`web_signin/` 提供了一个基于 **Flask + APScheduler** 的 Web 管理界面，用于在浏览器中扫码添加用户、配置多个时间点和查看签到日志。

### 本地直接运行

1. 安装 Web 版依赖（在项目根目录执行）：
   ```bash
   pip install -r web_signin/requirements.txt
   ```
2. （可选）复制配置模板：
   ```bash
   cd web_signin
   copy config.json.example config.json  # Windows
   # 或
   cp config.json.example config.json    # Linux / macOS
   ```
   如需自定义端口，可在 `config.json` 中填写 `"port": 9988`，或通过环境变量 `WEB_SIGNIN_PORT` 覆盖。
3. 启动 Web 服务：
   ```bash
   python web_signin/run.py
   ```
4. 在浏览器访问 `http://127.0.0.1:9988`。若修改了端口，请替换 `9988`。

### 两个页面

| 路径 | 用途 | 鉴权 |
| --- | --- | --- |
| `/` | 扫码添加用户、提交签到任务、查看自己的日志 | 无（靠扫码 token 定位到本人数据） |
| `/manage` | 查看 / 筛选 / 修改 / 删除所有任务与日志 | 管理员 Key |

首次打开 `/manage` 会提示「初始化管理员 Key」：设一个密码即可，随后自动登录，Key 只保存 SHA-256 哈希到 `web_signin/db/`。
之后每次打开都用这个 Key 登录，登录得到的 token 只存在当前标签页的 `sessionStorage`，关掉即失效。

也可以不用页面，直接走命令行：

```bash
# 首次初始化（只能调一次）
curl -X POST http://127.0.0.1:9988/api/admin/init \
  -H "Content-Type: application/json" -d '{"key":"你的管理员密码"}'

# 登录拿 token（默认 24 小时有效）
curl -X POST http://127.0.0.1:9988/api/admin/login \
  -H "Content-Type: application/json" -d '{"key":"你的管理员密码"}'

# 之后每个 /api/admin/* 请求带上 token
curl -H "X-Admin-Token: <token>" http://127.0.0.1:9988/api/admin/tasks
```

> 忘了 Key 就删掉 `web_signin/db/web_signin.db` 里的 `settings` 记录（或整个库文件）重新初始化——
> 注意这会一并删掉任务与日志，删之前先备份。

## 开发：跑测试

测试全程使用 mock，**不会发出真实网络请求，也不会产生真实签到**，可以放心反复运行。

```bash
pip install -r requirements-dev.txt
pytest
```

覆盖范围：`attendance` 的六种状态分支与多模块探测、`user_info` 的 Cookie 提取与页面解析、`notification` 的消息格式与超时、`config_manager` 的读写、`runner` 的退出码与班级回填、`signin_loop` 的时间计算、`web_signin` 调度器的时区与日志状态映射、全部 Flask 接口（任务创建/查询/日志、用户配置、管理员初始化/登录/token 鉴权/改删）、`auto_add_user` 的配置路径与日志脱敏，以及桌面端的数据目录判定、设置归一化、调度重试/暂停/跨点补跑、扫码 Cookie 提取和窗口构建冒烟。

当前共 **181 个用例**，其中 `tests/test_web_api.py` 用 Flask `test_client` + 临时 SQLite 覆盖后端接口；`tests/test_desktop_*.py` 覆盖桌面端逻辑，唯一需要图形环境的是 `test_desktop_app.py`（无桌面会话时会自动跳过）。

## 文件与隐私说明

- `data.json`、`.env`、`web_signin/config.json`、`web_signin/db/` 包含个人 Cookie、经纬度、推送 Key，
  **已全部写入 `.gitignore`，不会被提交**，请自行保管。
- 桌面版把同样的凭证写在 `data\data.json`（绿色版）或 `%USERPROFILE%\AutoSignBJMF\data.json`（单 exe），
  这两个位置同样不在仓库里；`dist\AutoSignBJMF-便携版.zip` 是干净的，不含任何凭证，可以直接发给别人。
- 签到失败时程序会在当前目录生成 `debug_html.txt`（页面快照，含真实姓名与班级 ID），同样已被忽略；
  扫码登录失败时还可能生成 `web_signin/debug_qr.html`，`auto_add_user.py` 会生成 `login_qrcode.png`（含二维码票据），
  这两个也已加入 `.gitignore`。**对外分享日志或截图前，请手动打码或删除其中的 Cookie、班级 ID、经纬度等隐私信息。**
- `auto_add_user.py` 在终端回显配置时会对 Cookie 与推送 Key 打码，但**整段 stdout 仍可能含姓名与班级**，
  贴日志到公开场合前请自行检查。
- `doc/` 下的示例截图已对真实经纬度（`img0.jpg`）与班级 ID（`img5.jpg`）打码；
  但 Git 历史中的旧版本仍是打码前的原图，彻底清除需要重写历史（`git filter-repo` + force push）。


## 注意事项

- 程序会自动检测并填充空的 class 字段。
- 签到二维码/Cookie 具有时效性，如果签到失败（提示 Cookie 无效），请重新运行 `auto_add_user.py` 更新凭证。
- 长时间没有运行（例如 Actions 被停用）后，务必先 `python BJMF.py` 手动验证一次 Cookie 是否还有效。

## 许可证

本项目基于 [MIT License](LICENSE) 开源。
