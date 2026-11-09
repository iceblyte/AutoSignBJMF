"""桌面端视觉令牌与控件风格。

与 Web 端 `web_signin/static/theme.css` 同一套值（见 design/ui-guidelines.md）。
tkinter 无法做圆角与阴影，这里用 1px 描边 + 纯色分层替代，禁止在 app.py 里写死颜色。
"""

import tkinter as tk
from tkinter import ttk

# 浅色令牌（深色主题后续按同一结构补一套）
C = {
    "bg": "#f5f5f7",
    "surface": "#ffffff",
    "surface2": "#fafafa",
    "muted": "#f4f4f5",
    "muted2": "#ececee",
    "border": "#e4e4e7",
    "border_strong": "#d4d4d8",
    "text": "#18181b",
    "text2": "#52525b",
    "text3": "#8b8b93",
    "accent": "#2563eb",
    "accent_hover": "#1d4ed8",
    "accent_soft": "#eff4ff",
    "success": "#15803d",
    "success_soft": "#ecfdf3",
    "warning": "#b45309",
    "warning_soft": "#fffaeb",
    "danger": "#b91c1c",
    "danger_soft": "#fef2f2",
    "brand": "#e72726",
    "white": "#ffffff",
}

FONT = ("Microsoft YaHei UI", 10)
FONT_SMALL = ("Microsoft YaHei UI", 9)
FONT_TITLE = ("Microsoft YaHei UI", 13, "bold")
FONT_HEAD = ("Microsoft YaHei UI", 10, "bold")
FONT_MONO = ("Cascadia Mono", 9)


def apply_theme(root):
    """切换 clam 主题并统一 ttk 控件外观。"""
    root.configure(bg=C["bg"])

    style = ttk.Style(root)
    try:
        style.theme_use("clam")
    except tk.TclError:
        pass

    style.configure("TFrame", background=C["bg"])
    style.configure("TLabel", background=C["bg"], foreground=C["text"], font=FONT)
    style.configure("Hint.TLabel", background=C["surface"], foreground=C["text3"], font=FONT_SMALL)
    style.configure("Status.TLabel", background=C["bg"], foreground=C["text2"], font=FONT_SMALL)
    style.configure("Warn.TLabel", background=C["surface"], foreground=C["warning"], font=FONT_SMALL)

    style.configure(
        "TEntry",
        fieldbackground=C["surface"],
        foreground=C["text"],
        bordercolor=C["border_strong"],
        lightcolor=C["border_strong"],
        darkcolor=C["border_strong"],
        insertcolor=C["text"],
        padding=(7, 5),
    )
    style.map(
        "TEntry",
        bordercolor=[("focus", C["accent"])],
        lightcolor=[("focus", C["accent"])],
        darkcolor=[("focus", C["accent"])],
    )
    style.configure("TSpinbox", fieldbackground=C["surface"], foreground=C["text"],
                    bordercolor=C["border_strong"], arrowcolor=C["text2"], padding=(5, 4))

    style.configure(
        "TButton",
        background=C["surface"],
        foreground=C["text"],
        bordercolor=C["border_strong"],
        lightcolor=C["surface"],
        darkcolor=C["surface"],
        focusthickness=0,
        relief="flat",
        padding=(12, 6),
        font=FONT,
    )
    style.map(
        "TButton",
        background=[("active", C["surface2"]), ("disabled", C["muted"])],
        foreground=[("disabled", C["text3"])],
    )

    style.configure(
        "Treeview",
        background=C["surface"],
        fieldbackground=C["surface"],
        foreground=C["text"],
        bordercolor=C["border"],
        borderwidth=0,
        rowheight=30,
        font=FONT_SMALL,
    )
    style.map(
        "Treeview",
        background=[("selected", C["accent_soft"])],
        foreground=[("selected", C["text"])],
    )
    style.configure(
        "Treeview.Heading",
        background=C["surface2"],
        foreground=C["text3"],
        bordercolor=C["border"],
        relief="flat",
        padding=(8, 6),
        font=FONT_SMALL,
    )
    style.map("Treeview.Heading", background=[("active", C["muted"])])

    style.configure(
        "TScrollbar",
        background=C["muted2"],
        troughcolor=C["surface"],
        bordercolor=C["surface"],
        arrowcolor=C["text3"],
        arrowsize=10,
        width=10,
    )
    style.map("TScrollbar", background=[("active", C["border_strong"])])

    style.configure("TCheckbutton", background=C["bg"], foreground=C["text2"], font=FONT_SMALL)
    style.map("TCheckbutton", background=[("active", C["bg"])])
    return style


def card(parent, title=None, subtitle=None, pad=14, fill="both"):
    """返回 (卡片外框, 内容容器)。外框用 1px 描边模拟卡片边界。"""
    outer = tk.Frame(
        parent,
        bg=C["surface"],
        bd=0,
        highlightthickness=1,
        highlightbackground=C["border"],
        highlightcolor=C["border"],
    )
    if title:
        head = tk.Frame(outer, bg=C["surface"])
        head.pack(fill="x", padx=pad, pady=(pad - 2, 0))
        tk.Label(head, text=title, bg=C["surface"], fg=C["text"], font=FONT_HEAD).pack(anchor="w")
        if subtitle:
            tk.Label(head, text=subtitle, bg=C["surface"], fg=C["text3"], font=FONT_SMALL).pack(
                anchor="w", pady=(1, 0)
            )
    body = tk.Frame(outer, bg=C["surface"])
    body.pack(fill=fill, expand=True, padx=pad, pady=(8, pad))
    return outer, body


def primary_button(parent, text, command, width=None):
    return tk.Button(
        parent,
        text=text,
        command=command,
        bg=C["accent"],
        fg=C["white"],
        activebackground=C["accent_hover"],
        activeforeground=C["white"],
        relief="flat",
        bd=0,
        highlightthickness=0,
        cursor="hand2",
        font=FONT,
        padx=14,
        pady=7,
        disabledforeground="#c7d2fe",
        width=width,
    )


def ghost_button(parent, text, command, width=None):
    return tk.Button(
        parent,
        text=text,
        command=command,
        bg=C["surface"],
        fg=C["text"],
        activebackground=C["surface2"],
        activeforeground=C["text"],
        relief="flat",
        bd=0,
        highlightthickness=1,
        highlightbackground=C["border_strong"],
        highlightcolor=C["border_strong"],
        cursor="hand2",
        font=FONT,
        padx=12,
        pady=6,
        width=width,
    )


def link_button(parent, text, command):
    return tk.Button(
        parent,
        text=text,
        command=command,
        bg=C["surface"],
        fg=C["accent"],
        activebackground=C["surface"],
        activeforeground=C["accent_hover"],
        relief="flat",
        bd=0,
        highlightthickness=0,
        cursor="hand2",
        font=FONT_SMALL,
        padx=2,
        pady=0,
    )


def pill(parent, text, kind="idle", dot=True):
    palette = {
        "ok": (C["success_soft"], C["success"]),
        "warn": (C["warning_soft"], C["warning"]),
        "err": (C["danger_soft"], C["danger"]),
        "idle": (C["muted"], C["text2"]),
    }[kind]
    bg, fg = palette
    label = tk.Label(
        parent,
        text=("● " if dot else "") + text,
        bg=bg,
        fg=fg,
        font=FONT_SMALL,
        padx=8,
        pady=2,
    )
    return label


def _logo_path():
    import sys
    from pathlib import Path

    base = getattr(sys, "_MEIPASS", None)
    if base:
        return Path(base) / "desktop" / "assets" / "logo.png"
    return Path(__file__).resolve().parent / "assets" / "logo.png"


def brand_mark(parent, size=28, bg=None):
    """品牌标识：优先加载 assets/logo.png，缺失时退化为纯色方块 + 对勾。"""
    bg = bg or C["surface"]
    try:
        from PIL import Image, ImageTk

        image = Image.open(_logo_path()).convert("RGBA")
        image = image.resize((size, size), Image.LANCZOS)
        photo = ImageTk.PhotoImage(image)
        label = tk.Label(parent, image=photo, bd=0, bg=bg, highlightthickness=0)
        label.image = photo  # 防止被回收
        return label
    except Exception:
        canvas = tk.Canvas(parent, width=size, height=size, bg=bg, highlightthickness=0, bd=0)
        canvas.create_rectangle(0, 0, size, size, fill=C["accent"], outline="")
        s = size
        canvas.create_line(s * 0.26, s * 0.52, s * 0.44, s * 0.70, fill="white", width=max(2, s // 10))
        canvas.create_line(s * 0.44, s * 0.70, s * 0.75, s * 0.33, fill="white", width=max(2, s // 10))
        return canvas


def hairline(parent, pady=0):
    line = tk.Frame(parent, bg=C["border"], height=1, bd=0, highlightthickness=0)
    return line
