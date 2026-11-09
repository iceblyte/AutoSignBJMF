"""桌面端主窗口：账号管理、时间点、坐标拾取、后台调度与托盘。"""

import os
import queue
import re
import threading
import webbrowser
from datetime import datetime

import tkinter as tk
from tkinter import messagebox, simpledialog, ttk

import desktop.autostart as autostart
import desktop.paths as paths
import desktop.settings as settings_store
import desktop.theme as theme
from desktop.qr_dialog import QRLoginDialog
from desktop.scheduler import DailyScheduler
from desktop.tray import TrayController
from utils.config_manager import load_config, save_config
from utils.runner import run_once
from utils.timepoints import humanize_delta, parse_times

APP_TITLE = "班级魔方自动签到"
AMAP_URL = "https://lbs.amap.com/tools/picker"
COORD_SPLIT = re.compile(r"[,\s，|、]+")


def parse_coord(text):
    """把 "39.90, 116.39" 解析成 ("39.90", "116.39")；失败返回 (None, None)。"""
    parts = [part for part in COORD_SPLIT.split((text or "").strip()) if part]
    if len(parts) < 2:
        return None, None
    return parts[0], parts[1]


class SigninApp:
    def __init__(self, root):
        self.root = root
        self.root.title(APP_TITLE)
        self.root.geometry("1080x800")
        self.root.minsize(960, 720)
        self._apply_window_icon()

        self.config_path = paths.config_path()
        self.settings_path = paths.settings_path()
        self.log_file = paths.log_dir() / f"app-{datetime.now():%Y%m%d}.log"
        self.settings = settings_store.load_settings(self.settings_path)
        self.config = {"students": []}
        self.students = []
        self.scheduler = None
        self.events = queue.Queue()
        self._busy = False
        self._tray = None
        self._hidden = False
        self._pump_id = None
        self._tick_id = None

        self._build_style()
        self._build_ui()
        self._load_students()
        self._load_settings_into_ui()

        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self._pump_id = self.root.after(150, self._pump)
        self._tick_id = self.root.after(1000, self._tick)

        self._log(f"数据目录：{paths.ensure_data_dir()}")
        self._start_tray()

        if self.students:
            self.root.after(600, lambda: self._start_scheduler(quiet=True))

    # ================= 界面搭建 =================

    def _apply_window_icon(self):
        """窗口与任务栏使用与 exe 相同的 logo，不设的话 Windows 会显示 Tk 默认羽毛图标。"""
        try:
            from PIL import Image, ImageTk

            image = Image.open(theme._logo_path()).convert("RGBA")
            image = image.resize((64, 64), Image.LANCZOS)
            self._window_icon = ImageTk.PhotoImage(image)
            self.root.iconphoto(True, self._window_icon)
        except Exception:
            pass

    def _build_style(self):
        theme.apply_theme(self.root)

    def _build_ui(self):
        outer = tk.Frame(self.root, bg=theme.C["bg"])
        outer.pack(fill="both", expand=True)

        self._build_topbar(outer)

        body = tk.Frame(outer, bg=theme.C["bg"])
        body.pack(fill="both", expand=True, padx=16, pady=(12, 14))

        side, side_body = theme.card(body, "账号", "扫码添加，可维护多个学生", pad=12)
        side.pack(side="left", fill="y")
        side.configure(width=262)
        side.pack_propagate(False)
        self._build_accounts(side_body)

        right = tk.Frame(body, bg=theme.C["bg"])
        right.pack(side="left", fill="both", expand=True, padx=(12, 0))

        self._build_detail(right)

        middle = tk.Frame(right, bg=theme.C["bg"])
        middle.pack(fill="both", expand=True, pady=(12, 0))
        self._build_schedule(middle)
        self._build_log(middle)

        self._build_controls(right)

    def _build_topbar(self, parent):
        bar = tk.Frame(parent, bg=theme.C["surface"])
        bar.pack(fill="x")
        tk.Frame(bar, bg=theme.C["border"], height=1).pack(side="bottom", fill="x")

        inner = tk.Frame(bar, bg=theme.C["surface"])
        inner.pack(fill="x", padx=16, pady=11)

        theme.brand_mark(inner, 30).pack(side="left")
        title_box = tk.Frame(inner, bg=theme.C["surface"])
        title_box.pack(side="left", padx=(10, 0))
        tk.Label(
            title_box, text="班级魔方自动签到",
            bg=theme.C["surface"], fg=theme.C["text"], font=theme.FONT_HEAD,
        ).pack(anchor="w")
        tk.Label(
            title_box, text="扫码一次，之后每天按时间自动签到",
            bg=theme.C["surface"], fg=theme.C["text3"], font=theme.FONT_SMALL,
        ).pack(anchor="w")

        self.status_var = tk.StringVar(value="未运行")
        self.status_pill = theme.pill(inner, "未运行", "idle")
        self.status_pill.pack(side="right")
        self.status_var.trace_add("write", lambda *_: self._sync_status_pill())

        self.autostart_var = tk.BooleanVar(value=autostart.is_autostart_enabled())
        ttk.Checkbutton(
            inner, text="开机自启", variable=self.autostart_var, command=self._toggle_autostart
        ).pack(side="right", padx=(0, 14))

    def _sync_status_pill(self):
        text = self.status_var.get()
        if text.startswith("运行中"):
            kind = "ok"
        elif "暂停" in text:
            kind = "warn"
        elif "异常" in text or "失败" in text:
            kind = "err"
        else:
            kind = "idle"
        bg, fg = {
            "ok": (theme.C["success_soft"], theme.C["success"]),
            "warn": (theme.C["warning_soft"], theme.C["warning"]),
            "err": (theme.C["danger_soft"], theme.C["danger"]),
            "idle": (theme.C["muted"], theme.C["text2"]),
        }[kind]
        self.status_pill.configure(text="● " + text, bg=bg, fg=fg)

    def _field(self, parent, label, variable, width=10, show=None):
        box = tk.Frame(parent, bg=theme.C["surface"])
        tk.Label(
            box, text=label, bg=theme.C["surface"], fg=theme.C["text3"], font=theme.FONT_SMALL
        ).pack(anchor="w")
        entry = ttk.Entry(box, textvariable=variable, width=width, show=show)
        entry.pack(anchor="w", pady=(4, 0))
        return box

    def _build_accounts(self, parent):
        holder = tk.Frame(parent, bg=theme.C["surface"])
        holder.pack(fill="both", expand=True)

        self.tree = ttk.Treeview(
            holder, columns=("name", "class_id"), show="headings", height=9, selectmode="browse"
        )
        for key, label, width in (
            ("name", "姓名", 118),
            ("class_id", "班级", 96),
        ):
            self.tree.heading(key, text=label, anchor="w")
            self.tree.column(key, width=width, anchor="w", minwidth=60, stretch=True)
        self.tree.pack(side="left", fill="both", expand=True)
        self.tree.bind("<<TreeviewSelect>>", lambda _event: self._on_select())

        scroll = ttk.Scrollbar(holder, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=scroll.set)
        scroll.pack(side="left", fill="y")

        actions = tk.Frame(parent, bg=theme.C["surface"])
        actions.pack(fill="x", pady=(10, 0))
        theme.primary_button(actions, "扫码添加 / 更新", self._open_qr_dialog).pack(fill="x")
        theme.ghost_button(actions, "删除选中", self._delete_student).pack(fill="x", pady=(6, 0))

    def _build_detail(self, parent):
        outer, body = theme.card(
            parent, "位置与通知", "选中账号后修改；签到位置就是这里的经纬度"
        )
        outer.pack(fill="x")

        row1 = tk.Frame(body, bg=theme.C["surface"])
        row1.pack(fill="x")
        self.lat_var = tk.StringVar()
        self.lng_var = tk.StringVar()
        self.acc_var = tk.StringVar(value="30")
        self._field(row1, "纬度", self.lat_var, 12).pack(side="left")
        self._field(row1, "经度", self.lng_var, 12).pack(side="left", padx=(10, 0))
        self._field(row1, "精度", self.acc_var, 6).pack(side="left", padx=(10, 0))
        theme.ghost_button(row1, "打开高德坐标拾取器", self._open_amap).pack(side="left", padx=(12, 0), pady=(30, 0))
        theme.primary_button(row1, "保存", self._save_student).pack(side="right", pady=(30, 0))

        row2 = tk.Frame(body, bg=theme.C["surface"])
        row2.pack(fill="x", pady=(10, 0))
        self.wx_var = tk.StringVar()
        self.qq_var = tk.StringVar()
        self._field(row2, "微信通知 Key", self.wx_var, 22).pack(side="left")
        self._field(row2, "QQ 通知 Key", self.qq_var, 22).pack(side="left", padx=(10, 0))

        tk.Label(
            body,
            text="班级与 Cookie 由扫码自动填充；通知 Key 留空表示不推送。",
            bg=theme.C["surface"], fg=theme.C["text3"], font=theme.FONT_SMALL,
        ).pack(anchor="w", pady=(8, 0))

    def _build_schedule(self, parent):
        outer, body = theme.card(parent, "每天执行时间", "可加多条；重试填 0 表示失败后不再重试")
        outer.pack(side="left", fill="y")
        outer.configure(width=382)
        outer.pack_propagate(False)

        content = tk.Frame(body, bg=theme.C["surface"])
        content.pack(fill="both", expand=True)

        self.times_list = tk.Listbox(
            content,
            height=6,
            activestyle="none",
            exportselection=False,
            bg=theme.C["surface"], fg=theme.C["text"],
            selectbackground=theme.C["accent_soft"], selectforeground=theme.C["text"],
            bd=0, relief="flat", highlightthickness=1,
            highlightbackground=theme.C["border"], highlightcolor=theme.C["border"],
            font=theme.FONT_MONO,
        )
        self.times_list.pack(side="left", fill="both", expand=True)
        times_scroll = ttk.Scrollbar(content, orient="vertical", command=self.times_list.yview)
        self.times_list.configure(yscrollcommand=times_scroll.set)
        times_scroll.pack(side="left", fill="y")

        editor = tk.Frame(body, bg=theme.C["surface"])
        editor.pack(fill="x", pady=(8, 0))
        self.new_time_var = tk.StringVar(value="07:30")
        ttk.Entry(editor, textvariable=self.new_time_var, width=9).pack(side="left")
        theme.primary_button(editor, "添加", self._add_time).pack(side="left", padx=(6, 0))
        theme.ghost_button(editor, "删除选中", self._remove_time).pack(side="left", padx=(6, 0))

        retry = tk.Frame(body, bg=theme.C["surface"])
        retry.pack(fill="x", side="bottom", pady=(10, 0))
        self.retry_attempts_var = tk.StringVar(value=str(self.settings["retry_attempts"]))
        self.retry_delay_var = tk.StringVar(value=str(self.settings["retry_delay_minutes"]))
        tk.Label(
            retry, text="失败后重试", bg=theme.C["surface"], fg=theme.C["text3"], font=theme.FONT_SMALL
        ).pack(side="left")
        ttk.Spinbox(retry, from_=1, to=10, width=4, textvariable=self.retry_attempts_var).pack(
            side="left", padx=(6, 6)
        )
        tk.Label(
            retry, text="次，间隔", bg=theme.C["surface"], fg=theme.C["text3"], font=theme.FONT_SMALL
        ).pack(side="left", padx=(0, 2))
        ttk.Spinbox(retry, from_=0, to=120, width=4, textvariable=self.retry_delay_var).pack(
            side="left", padx=(6, 6)
        )
        tk.Label(
            retry, text="分钟",
            bg=theme.C["surface"], fg=theme.C["text3"], font=theme.FONT_SMALL,
        ).pack(side="left")

    def _build_log(self, parent):
        outer, body = theme.card(parent, "运行日志", "关闭窗口会缩到托盘，签到继续执行")
        outer.pack(side="left", fill="both", expand=True, padx=(12, 0))

        holder = tk.Frame(body, bg=theme.C["surface2"])
        holder.pack(fill="both", expand=True)
        self.log_text = tk.Text(
            holder,
            height=8,
            wrap="word",
            state="disabled",
            bg=theme.C["surface2"], fg=theme.C["text2"],
            bd=0, relief="flat", highlightthickness=0,
            padx=10, pady=8,
            font=theme.FONT_MONO,
            insertbackground=theme.C["text"],
        )
        self.log_text.pack(side="left", fill="both", expand=True)
        scroll = ttk.Scrollbar(holder, orient="vertical", command=self.log_text.yview)
        self.log_text.configure(yscrollcommand=scroll.set)
        scroll.pack(side="left", fill="y")

    def _build_controls(self, parent):
        box = tk.Frame(parent, bg=theme.C["bg"])
        box.pack(fill="x", pady=(12, 0))

        self.btn_start = theme.primary_button(box, "启动后台运行", self._start_scheduler)
        self.btn_start.pack(side="left")
        self.btn_stop = theme.ghost_button(box, "停止", self._stop_scheduler)
        self.btn_stop.configure(state="disabled")
        self.btn_stop.pack(side="left", padx=(6, 0))
        self.btn_run = theme.ghost_button(box, "立即签到一次", self._run_now)
        self.btn_run.pack(side="left", padx=(6, 0))
        portable = paths.is_portable()
        tk.Label(
            box,
            text="数据：便携目录 data\\" if portable else "数据：用户目录",
            bg=theme.C["bg"], fg=theme.C["text3"], font=theme.FONT_SMALL,
        ).pack(side="left", padx=(14, 4))
        theme.link_button(box, "打开数据目录", self._open_data_dir).pack(side="left")

    # ================= 数据 =================

    def _load_students(self):
        loaded = load_config(str(self.config_path))
        if isinstance(loaded, dict) and loaded:
            self.config = loaded
        else:
            self.config = {"students": []}
        self.students = self.config.setdefault("students", [])
        self._refresh_tree()
        if self.students and not self.tree.selection():
            self.tree.selection_set("0")

    def _refresh_tree(self):
        for item in self.tree.get_children():
            self.tree.delete(item)
        for index, student in enumerate(self.students):
            self.tree.insert(
                "",
                "end",
                iid=str(index),
                values=(student.get("name", ""), student.get("class", "")),
            )

    def _selected_index(self):
        selection = self.tree.selection()
        if not selection:
            return None
        try:
            return int(selection[0])
        except ValueError:
            return None

    def _on_select(self):
        index = self._selected_index()
        if index is None or index >= len(self.students):
            return
        student = self.students[index]
        self.lat_var.set(student.get("lat", ""))
        self.lng_var.set(student.get("lng", ""))
        self.acc_var.set(student.get("acc", "30") or "30")
        self.wx_var.set(student.get("WXKey", ""))
        self.qq_var.set(student.get("QQKey") or student.get("QmsgKEY", ""))

    def _persist_students(self):
        self.config["students"] = self.students
        save_config(self.config, str(self.config_path))

    def _save_student(self):
        index = self._selected_index()
        if index is None:
            messagebox.showinfo(APP_TITLE, "请先在列表里选中一个账号。")
            return

        lat = self.lat_var.get().strip()
        lng = self.lng_var.get().strip()
        if not lat or not lng:
            messagebox.showwarning(APP_TITLE, "经纬度不能为空，可以点「打开高德坐标拾取器」复制后粘贴。")
            return

        student = self.students[index]
        student["lat"] = lat
        student["lng"] = lng
        student["acc"] = self.acc_var.get().strip() or "30"
        student["WXKey"] = self.wx_var.get().strip()
        student["QmsgKEY"] = self.qq_var.get().strip()

        self._persist_students()
        self._refresh_tree()
        self.tree.selection_set(str(index))
        self._log(f"已保存 {student.get('name', '')} 的位置与通知设置")
        messagebox.showinfo(APP_TITLE, "已保存。")

    def _delete_student(self):
        index = self._selected_index()
        if index is None:
            messagebox.showinfo(APP_TITLE, "请先选中要删除的账号。")
            return
        name = self.students[index].get("name", "该账号")
        if not messagebox.askyesno(APP_TITLE, f"确定删除「{name}」吗？"):
            return
        del self.students[index]
        self._persist_students()
        self._refresh_tree()
        self._log(f"已删除账号 {name}")

    # ================= 扫码 =================

    def _open_qr_dialog(self):
        QRLoginDialog(self.root, self._on_qr_success)

    def _on_qr_success(self, cookie, user_info, class_info):
        name = (user_info or {}).get("name") or "未命名"
        class_id = (class_info or {}).get("class_id") or ""
        if class_id == "未找到":
            class_id = ""

        existing = next(
            (index for index, student in enumerate(self.students) if student.get("name") == name), None
        )
        if existing is None:
            student = {
                "name": name,
                "class": class_id,
                "lat": self.lat_var.get().strip(),
                "lng": self.lng_var.get().strip(),
                "acc": self.acc_var.get().strip() or self.settings.get("acc", "30"),
                "cookie": cookie,
                "QmsgKEY": self.qq_var.get().strip(),
                "WXKey": self.wx_var.get().strip(),
            }
            self.students.append(student)
        else:
            student = self.students[existing]
            student["cookie"] = cookie
            student["class"] = class_id or student.get("class", "")
            if not student.get("lat"):
                student["lat"] = self.lat_var.get().strip()
            if not student.get("lng"):
                student["lng"] = self.lng_var.get().strip()

        self._persist_students()
        self._refresh_tree()
        index = self.students.index(student)
        self.tree.selection_set(str(index))
        self._log(f"扫码成功：{name}（班级 {class_id or '未知'}）")
        messagebox.showinfo(APP_TITLE, f"已{'更新' if existing is not None else '添加'}账号：{name}")

    # ================= 时间点 =================

    def _load_settings_into_ui(self):
        self.times_list.delete(0, "end")
        for item in self.settings["times"]:
            self.times_list.insert("end", item)
        self.retry_attempts_var.set(str(self.settings["retry_attempts"]))
        self.retry_delay_var.set(str(self.settings["retry_delay_minutes"]))

    def _add_time(self):
        raw = self.new_time_var.get().strip()
        try:
            parsed = parse_times(raw)[0]
        except ValueError as error:
            messagebox.showerror(APP_TITLE, str(error))
            return

        current = [self.times_list.get(i) for i in range(self.times_list.size())]
        if parsed in current:
            messagebox.showinfo(APP_TITLE, f"{parsed} 已经在列表里了。")
            return

        self.times_list.insert("end", parsed)
        if not self._save_schedule():
            self.times_list.delete(self.times_list.size() - 1)
            return
        self._log(f"已添加执行时间 {parsed}")

    def _remove_time(self):
        selection = self.times_list.curselection()
        if not selection:
            return
        if self.times_list.size() <= 1:
            messagebox.showwarning(APP_TITLE, "至少要保留一个执行时间。")
            return

        value = self.times_list.get(selection[0])
        self.times_list.delete(selection[0])
        if not self._save_schedule():
            return
        self._log(f"已删除执行时间 {value}")

    def _save_schedule(self):
        try:
            times = parse_times([self.times_list.get(i) for i in range(self.times_list.size())])
        except ValueError as error:
            messagebox.showerror(APP_TITLE, str(error))
            return False

        self.settings["times"] = times
        self.settings["retry_attempts"] = self._safe_int(self.retry_attempts_var.get(), 3)
        self.settings["retry_delay_minutes"] = self._safe_int(self.retry_delay_var.get(), 5)
        self.settings = settings_store.save_settings(self.settings, str(self.settings_path))
        self._load_settings_into_ui()
        return True

    @staticmethod
    def _safe_int(value, fallback):
        try:
            return int(value)
        except (TypeError, ValueError):
            return fallback

    # ================= 调度 =================

    def _start_scheduler(self, quiet=False):
        if not self.students:
            if not quiet:
                messagebox.showwarning(APP_TITLE, "还没有账号，请先点「扫码添加 / 更新」。")
            return
        if not self._save_schedule():
            return

        if self.scheduler is not None:
            self.scheduler.stop()

        self.scheduler = DailyScheduler(
            lambda: run_once(str(self.config_path)),
            lambda: list(self.settings["times"]),
            retry_attempts=self.settings["retry_attempts"],
            retry_delay_seconds=self.settings["retry_delay_minutes"] * 60,
            on_event=self._on_scheduler_event,
        )
        self.scheduler.start()

        self.btn_start.configure(state="disabled")
        self.btn_stop.configure(state="normal")
        self._log(f"后台调度已启动，每天 {', '.join(self.settings['times'])} 执行")

    def _stop_scheduler(self):
        if self.scheduler is not None:
            self.scheduler.stop()
            self.scheduler = None
        self.btn_start.configure(state="normal")
        self.btn_stop.configure(state="disabled")
        self.status_var.set("已停止")
        self._log("后台调度已停止")

    def _on_scheduler_event(self, name, payload):
        self.events.put(("sched", name, payload))

    def _run_now(self):
        if self._busy:
            return
        if not self.students:
            messagebox.showwarning(APP_TITLE, "还没有账号，请先扫码添加。")
            return
        self._busy = True
        self.btn_run.configure(state="disabled")
        self._log("开始手动签到…")

        def worker():
            try:
                code = run_once(str(self.config_path))
            except Exception as error:  # run_once 内部已兜底，这里只防万一
                self.events.put(("log", f"手动签到异常：{error}"))
                code = 1
            self.events.put(("manual_done", code))

        threading.Thread(target=worker, name="bjmf-manual", daemon=True).start()

    # ================= 系统交互 =================

    def _open_amap(self):
        webbrowser.open(AMAP_URL)
        text = simpledialog.askstring(
            "粘贴坐标",
            "浏览器已打开高德坐标拾取器。\n在地图上选好位置后复制坐标，粘贴到这里（例如 39.9087,116.3975）：",
            parent=self.root,
        )
        if not text:
            return
        lat, lng = parse_coord(text)
        if lat is None:
            messagebox.showwarning(APP_TITLE, "没能识别出经纬度，请粘贴成「纬度 经度」的形式。")
            return
        self.lat_var.set(lat)
        self.lng_var.set(lng)
        self._log(f"已填入坐标 {lat}, {lng}，记得点「保存」")

    def _open_data_dir(self):
        target = paths.ensure_data_dir()
        try:
            os.startfile(str(target))  # noqa: S606  Windows 专用
        except Exception as error:
            messagebox.showinfo(APP_TITLE, f"数据目录：{target}\n打开失败：{error}")

    def _toggle_autostart(self):
        enabled = autostart.set_autostart(self.autostart_var.get())
        if not enabled:
            self.autostart_var.set(False)
            messagebox.showwarning(APP_TITLE, "设置开机自启失败（可能被安全软件拦截）。")
            return
        self._log(f"开机自启已{'开启' if self.autostart_var.get() else '关闭'}")

    # ================= 托盘 =================

    def _start_tray(self):
        self._tray = TrayController(lambda action: self.events.put((action,)))
        if not self._tray.start():
            self._tray = None
            self._log("托盘不可用，关闭窗口将直接退出")

    def _toggle_pause(self):
        if self.scheduler is None:
            return
        if self.scheduler.paused:
            self.scheduler.resume()
            self._log("已恢复调度")
        else:
            self.scheduler.pause()
            self._log("已暂停调度（不会在到点时执行）")

    def _show_window(self):
        self._hidden = False
        self.root.deiconify()
        self.root.lift()
        try:
            self.root.focus_force()
        except tk.TclError:
            pass

    def destroy(self):
        """取消定时回调、停掉调度与托盘，再交给调用方销毁窗口。"""
        for attr in ("_pump_id", "_tick_id"):
            after_id = getattr(self, attr, None)
            if after_id is not None:
                try:
                    self.root.after_cancel(after_id)
                except tk.TclError:
                    pass
                setattr(self, attr, None)
        if self.scheduler is not None:
            self.scheduler.stop()
            self.scheduler = None
        if self._tray is not None:
            self._tray.stop()
            self._tray = None

    def _quit(self):
        self.destroy()
        self.root.destroy()

    def _on_close(self):
        if self._tray is not None:
            self.root.withdraw()
            self._hidden = True
            self._tray.notify("已缩到托盘继续运行，右键图标可以退出")
            return
        if messagebox.askyesno(APP_TITLE, "关闭窗口会停止后台签到，确定退出吗？"):
            self._quit()

    # ================= 事件循环 =================

    def _pump(self):
        try:
            while True:
                event = self.events.get_nowait()
                self._handle_event(event)
        except queue.Empty:
            pass
        self._pump_id = self.root.after(150, self._pump)

    def _handle_event(self, event):
        kind = event[0]

        if kind == "_append":
            self._append_log(event[1])
            return

        if kind == "log":
            self._log(event[1])
            return

        if kind == "sched":
            self._handle_scheduler_event(event[1], event[2])
            return

        if kind == "manual_done":
            self._busy = False
            self.btn_run.configure(state="normal")
            self._log("手动签到结束，" + ("成功" if event[1] == 0 else f"退出码 {event[1]}，请查看日志"))
            return

        if kind == "show":
            self._show_window()
            return

        if kind == "run_now":
            self._run_now()
            return

        if kind == "toggle_pause":
            self._toggle_pause()
            return

        if kind == "quit":
            self._quit()
            return

    def _handle_scheduler_event(self, name, payload):
        if name == "next_run":
            at = payload.get("at")
            self._log(f"下次执行：{at:%Y-%m-%d %H:%M:%S}")
            return

        if name == "run_start":
            self._log(f"开始签到（第 {payload['attempt']}/{payload['total']} 次尝试）")
            return

        if name == "run_done":
            code = payload.get("code", 1)
            if code == 0:
                self._log("本次签到完成")
                if self._tray is not None and self._hidden:
                    self._tray.notify("本次签到完成")
            else:
                self._log(f"本次签到失败（退出码 {code}）")
                if self._tray is not None and self._hidden:
                    self._tray.notify(f"本次签到失败（退出码 {code}）")
            return

        if name == "skipped":
            self._log("到点但调度处于暂停状态，本次跳过")
            return

        if name == "error":
            self._log(f"调度出错：{payload.get('message', '')}")
            return

        if name == "stopped":
            self.status_var.set("已停止")
            return

        if name == "paused":
            self.status_var.set("已暂停（到点不执行）")
            return

        if name == "resumed":
            self._log("调度已恢复")
            return

    def _tick(self):
        if self.scheduler is not None and self.scheduler.running:
            if self.scheduler.paused:
                self.status_var.set("已暂停（到点不执行）")
            elif self.scheduler.next_run_at is not None:
                remaining = (self.scheduler.next_run_at - datetime.now()).total_seconds()
                self.status_var.set(
                    f"运行中 · 下次执行 {self.scheduler.next_run_at:%m-%d %H:%M:%S}"
                    f"（还有 {humanize_delta(remaining)}）"
                )
            else:
                self.status_var.set("运行中 · 正在计算下次执行时间…")
        elif self.status_var.get().startswith("运行中"):
            self.status_var.set("未运行")
        self._tick_id = self.root.after(1000, self._tick)

    # ================= 日志 =================

    def _log(self, message):
        self.events.put(("_append", message))

    def _append_log(self, message):
        line = f"[{datetime.now():%H:%M:%S}] {message}"
        self.log_text.configure(state="normal")
        self.log_text.insert("end", line + "\n")
        self.log_text.see("end")
        self.log_text.configure(state="disabled")
        try:
            with open(self.log_file, "a", encoding="utf-8") as handle:
                handle.write(f"{datetime.now():%Y-%m-%d %H:%M:%S} {message}\n")
        except OSError:
            pass

    def post_pipe_output(self, text):
        """给 sys.stdout 兜底用：把 print 的内容转到日志面板。"""
        self.events.put(("_append", text))
