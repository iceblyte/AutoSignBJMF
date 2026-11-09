# UI 规范（三端统一）

> 适用范围：`web_signin/static/index.html`（用户端）、`web_signin/static/manage.html`（管理端）、
> `desktop/`（tkinter 桌面端）。三端共用同一套令牌，只允许在「密度」上不同，不允许在「颜色 / 圆角 / 字号层级」上分叉。
>
> 可交互原型：`design/ui-prototype.html`（顶栏切换三端、右上切换明暗）。

## 1. 视觉来源与取舍

| 参考 | 取什么 | 不取什么 |
| --- | --- | --- |
| ElegantClipboard（GPUI kit） | zinc 中性色阶、hairline 描边、可配圆角、系统中文字体 | 毛玻璃 / Mica 材质（Web 与 tkinter 都无法可靠还原） |
| dbx（Tauri + shadcn-vue） | 紧凑桌面密度、侧栏 + 工具条 + 表格、状态 pill、克制的强调色 | 代码编辑器、多标签页等重型桌面范式 |
| Apple Design | 字号层级、留白节奏、单一强调色、克制的阴影 | 营销页式大标题与大幅留白（这是工具，不是官网） |

一句话：**中性灰画布 + 白色卡片 + 1px 描边 + 10~12px 圆角 + 一个蓝色强调色 + 极轻阴影**。
阴影只用来区分层级，不用来装饰；所有分割优先用 1px 描边而不是阴影。

## 2. 颜色令牌

| 令牌 | 浅色 | 深色 | 用途 |
| --- | --- | --- | --- |
| `bg` | `#f5f5f7` | `#131519` | 页面画布 |
| `surface` | `#ffffff` | `#1f2228` | 卡片、面板、窗口主体 |
| `surface-2` | `#fafafa` | `#1b1e23` | 侧栏、表头、日志区、次级面板 |
| `muted` | `#f4f4f5` | `#262a31` | chip、hover 底色、segmented 轨道 |
| `border` | `#e4e4e7` | `#31363f` | 1px 描边、表格分隔线 |
| `border-strong` | `#d4d4d8` | `#3f4650` | 输入框、次级按钮描边 |
| `text` | `#18181b` | `#f2f3f5` | 标题、正文主色 |
| `text-2` | `#52525b` | `#b5becb` | 次级正文、标签 |
| `text-3` | `#8b8b93` | `#8b95a3` | 说明文字、占位符、表头 |
| `accent` | `#2563eb` | `#4b8bf5` | 主按钮、链接、选中态 |
| `accent-soft` | `#eff4ff` | `#16233b` | 图标底、选中 chip 底色 |
| `success` | `#0d9488` | `#2dd4bf` | 成功状态（青绿，呼应 logo） |
| `warning` | `#b45309` | `#fbbf24` | Cookie 过期 / 需处理 |
| `danger` | `#b91c1c` | `#f87171` | 删除、失败 |

状态色必须**同时**配 `*-soft` 作为 pill 底色（浅色 `#ecfdf3` / `#fffaeb` / `#fef2f2`），
不允许直接用饱和色做大面积背景。

## 3. 字体与层级

- 字族：`-apple-system, "SF Pro Text", "PingFang SC", "Microsoft YaHei UI", "Segoe UI"`；
  等宽（Cookie、坐标、日志、时间）：`ui-monospace, "Cascadia Mono", "Consolas"`。
- 字重只用 4 档：450 正文 / 500 标签 / 550 控件 / 590 卡片标题 / 620 页面标题。
- 字号：

| 场景 | 字号 | 字重 |
| --- | --- | --- |
| 页面标题 | 20 | 640，`-0.02em` |
| 卡片标题 | 14 | 590 |
| 正文 / 输入值 | 13.5 | 450 |
| 按钮 / 导航 | 13 | 550 |
| 标签 / 次级说明 | 12.5 | 550 / 450 |
| 表头 / 辅助信息 | 11.5~12 | 590 / 500 |
| 等宽日志 | 11.5 | 400，行高 1.65 |

- 中文字距统一 `-0.006em`，标题 `-0.02em`；数字用 `font-variant-numeric: tabular-nums`。

## 4. 间距、圆角、控件尺寸

- 间距基于 4px 栅格：8 / 12 / 16 / 18 / 24。卡片内边距 18px，卡片间距 16px，页面左右 28px。
- 圆角：`6` 小控件 / `8` 按钮与输入框 / `10` 内层面板 / `12` 卡片 / `16` 窗口与模态。
- 控件高度：

| 控件 | 高度 |
| --- | --- |
| 主按钮、输入框 | 34 |
| 工具条内的按钮 / 搜索 / 图标按钮 | 32（同一行必须等高） |
| 小按钮 | 28 |
| chip | 26 |
| pill | 24 |
| 开关 | 36 × 21 |

- 表格行高 44~56，表头 11.5px 灰字，行之间只用 1px 下边框，hover 用 `surface-2` 铺底，不用斑马纹。

## 5. 品牌标识

- 素材：`web_signin/static/logo.png`（512，透明底）与 `desktop/assets/logo.png`（256，同一张图）。
  图形取自班级魔方官方标识（蓝色定位针 + 等轴测立方体 + 青绿对勾 + 环绕轨道），已重绘为透明底版本，
  不再使用代码临摹的近似图形。
- 用法：
  - Web 顶栏 26px；管理端侧栏 22px；均为 `object-fit: contain`，不加圆角与投影（透明底直接贴合背景）。
  - 桌面端顶栏 30px，由 `desktop/theme.brand_mark()` 通过 `desktop/assets/logo.png` 加载，加载失败时才退化为纯色方块。
  - exe / 托盘图标由 `build/make_icon.py` 从同一张 PNG 生成 `build/app.ico`，保证任务栏、托盘、窗口内完全一致。
- 禁止：拉伸变形、加描边或外发光、在深色背景上再垫白色底板（PNG 已含透明通道）。
- 打包：`build/build.ps1` 通过 `--add-data` 把 `desktop/assets` 一并打进 exe，缺少该参数会导致 exe 内退化为占位图形。

## 6. 三端映射

| 组件 | 用户端（Web） | 管理端（Web） | 桌面端（tkinter） |
| --- | --- | --- | --- |
| 画布 | `bg` 灰底 + 居中 1180 容器 | 白色外壳 + 左侧栏 | 窗口 = `surface`，侧栏 `surface-2` |
| 导航 | 顶部品牌 + 状态 pill + 「管理端」 | 左侧 224px 侧栏，选中项白底 + 描边 | 左侧 232px 账号列表，选中项白底 + 描边 |
| 主操作 | 「保存并启用」主按钮 | 「新建任务」主按钮 | 「立即签到」主按钮 |
| 统计 | 3 张独立卡片 | 面板内 `metric-strip`（无卡片，避免套娃） | 底部状态栏文字 |
| 列表 | 日志行（时间 / 内容 / 状态） | 表格（学生 / 时间 / 状态 / 操作） | 账号行（头像 / 姓名 / 状态点） |
| 日志 | 卡片内行列表 | 面板内 `<pre>` 等宽 | `panel` 内等宽文本区 |
| 对话框 | 遮罩 + 居中卡片，圆角 16 | 同左 | `Toplevel` + 白底 Frame（圆角不可得，用 1px 描边替代） |

**状态表达规则**：有文字位置时用「pill = 圆点 + 文字」；空间极窄（桌面侧栏行）时用纯圆点
+ 3px 同色光晕，不加文字也不加 pill 背景。禁止出现「只有一个点、没有任何语义」的空胶囊。

## 7. tkinter 落地约束

tkinter 无法做圆角、阴影、模糊，落地时按下面的降级规则，不要试图用 Canvas 硬画：

1. `ttk.Style().theme_use("clam")` 作为基座，逐项覆写 `TFrame / TLabel / TButton / TEntry / Treeview`。
2. 颜色全部从 `desktop/theme.py` 的常量取，与本文第 2 节一一对应；不允许在 `app.py` 里写死 hex。
3. 卡片用 `tk.Frame(bg=surface, highlightbackground=border, highlightthickness=1)` 模拟描边；
   圆角降级为直角，但描边宽度必须保持 1px。
4. 主按钮用 `tk.Button`（`relief="flat"`、`activebackground` 略深）+ `cursor="hand2"`；
   次级按钮同底色 `surface` + 1px 描边；禁止使用系统默认 3D 边框。
5. 字体：`("Microsoft YaHei UI", 10)` 正文、`("Microsoft YaHei UI", 9)` 辅助、
   `("Cascadia Mono", 9)` 日志；标题 11~12 加粗。
6. 状态 pill 用 `tk.Label(bg=soft, fg=success, padx=8, pady=2)`；圆点用 8px `Canvas` 圆或 `●` 字符。
7. 明暗主题：先实现浅色（默认），深色作为第二阶段；两套值都写进 `theme.py`，用 `theme.apply(root, dark)` 切换。

## 8. 落地步骤

1. 抽出 `web_signin/static/theme.css`（令牌 + 基础组件），两个页面引用同一份，删除各自的重复样式。
2. 按原型重排 `index.html`：顶栏 / 统计 / 左表单 + 右扫码，保留现有 JS 接口调用与元素 id。
3. 按原型重排 `manage.html`：侧栏 + 工具条 + 指标条 + 表格 + 日志面板，保留 `adminFetch` 与鉴权遮罩。
4. 新增 `desktop/theme.py`，把 `app.py` / `qr_dialog.py` 里散落的颜色、字体、`ttk.Style` 集中；
   侧栏行、卡片、主按钮、pill 按第 5、6 节重写。
5. 回归：`pytest` 全绿；三端各截一张图与原型比对；打包 exe 冒烟（`--smoke`）。

> 当前状态：第 1~4 步已完成（`theme.css` / `theme.js`、两个 Web 页面、`desktop/theme.py`），
> 184 个用例通过。改动尚未打包成新的 exe，需要时执行 `build\build.ps1`。
