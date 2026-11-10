"""无界面的桌面端服务层。

WebView 前端通过 pywebview 的 js_api 调这里的方法，返回纯 dict；
tkinter 版（desktop/app.py）仍然保留，两者共用同一批底层模块。
"""

import base64
import sys
import threading
from collections import deque
from datetime import datetime

import desktop.autostart as autostart
import desktop.paths as paths
import desktop.settings as settings_store
from desktop.qr_client import QRLoginClient
from desktop.scheduler import DailyScheduler
from utils.config_manager import load_config, save_config
from utils.runner import run_once
from utils.timepoints import humanize_delta, parse_times


class LogBuffer:
    """收集运行日志，供前端轮询；同时保留原始 stdout 行为。"""

    def __init__(self, passthrough=None, maxlen=600):
        self._lines = deque(maxlen=maxlen)
        self._lock = threading.Lock()
        self._passthrough = passthrough
        self._buffer = ""

    def write(self, text):
        if not text:
            return 0
        if self._passthrough is not None:
            try:
                self._passthrough.write(text)
            except Exception:
                pass
        with self._lock:
            self._buffer += text
            while "\n" in self._buffer:
                line, self._buffer = self._buffer.split("\n", 1)
                if line.strip():
                    self._lines.append(line.rstrip())
        return len(text)

    def flush(self):
        if self._passthrough is not None:
            try:
                self._passthrough.flush()
            except Exception:
                pass

    def isatty(self):
        return False

    def snapshot(self):
        with self._lock:
            return list(self._lines)


class DesktopService:
    def __init__(self):
        self.config_path = paths.config_path()
        self.settings_path = paths.settings_path()
        self.students = []
        self.settings = settings_store.load_settings(self.settings_path)
        self.selected = 0
        self.scheduler = None
        self.status_text = "未运行"
        self.next_run_at = None
        self.last_error = None
        self._qr_client = None
        self._qr_state = "idle"
        self._lock = threading.RLock()
        self.log = LogBuffer(passthrough=sys.__stdout__)
        self._install_log_capture()
        self.load()

    # ---------- 基础设施 ----------

    def _install_log_capture(self):
        sys.stdout = self.log
        sys.stderr = self.log

    def load(self):
        with self._lock:
            loaded = load_config(str(self.config_path))
            self.students = loaded.get("students", []) if isinstance(loaded, dict) else []
            self.selected = 0 if self.students else -1
            self.settings = settings_store.load_settings(self.settings_path)
        self.log.write(f"数据目录：{paths.ensure_data_dir()}\n")

    def _save_config(self):
        save_config({"students": self.students}, str(self.config_path))

    def _save_settings(self):
        self.settings = settings_store.save_settings(self.settings, self.settings_path)

    # ---------- 状态 ----------

    def state(self):
        student = self.students[self.selected] if 0 <= self.selected < len(self.students) else None
        countdown = ""
        if self.next_run_at:
            countdown = humanize_delta(self.next_run_at - datetime.now())
        return {
            "students": [
                {"name": s.get("name", ""), "class_id": s.get("class", "")} for s in self.students
            ],
            "selected": self.selected,
            "student": dict(student) if student else None,
            "settings": {
                "times": list(self.settings.get("times", [])),
                "retry_attempts": self.settings.get("retry_attempts", 3),
                "retry_delay_minutes": self.settings.get("retry_delay_minutes", 5),
                "autostart": autostart.is_autostart_enabled(),
            },
            "status": {
                "text": self.status_text,
                "running": bool(self.scheduler and self.scheduler.running),
                "paused": bool(self.scheduler and self.scheduler.paused),
                "next": self.next_run_at.strftime("%H:%M:%S") if self.next_run_at else "",
                "countdown": countdown,
            },
            "data": {
                "dir": str(paths.ensure_data_dir()),
                "portable": paths.is_portable(),
            },
            "qr": {"state": self._qr_state},
        }

    def logs(self):
        return self.log.snapshot()[-200:]

    def tick(self):
        """前端轻量轮询：只回状态与日志，不重传表单。"""
        countdown = ""
        if self.next_run_at:
            countdown = humanize_delta(self.next_run_at - datetime.now())
        return {
            "text": self.status_text,
            "running": bool(self.scheduler and self.scheduler.running),
            "paused": bool(self.scheduler and self.scheduler.paused),
            "next": self.next_run_at.strftime("%H:%M:%S") if self.next_run_at else "",
            "countdown": countdown,
            "qr_state": self._qr_state,
            "logs": self.log.snapshot()[-80:],
        }

    # ---------- 账号 ----------

    def select_student(self, index):
        try:
            index = int(index)
        except (TypeError, ValueError):
            return self.state()
        if 0 <= index < len(self.students):
            self.selected = index
        return self.state()

    def save_student(self, payload):
        """payload 里只取界面允许改的字段，Cookie 不在其中。"""
        payload = payload or {}
        student = self.students[self.selected] if 0 <= self.selected < len(self.students) else None
        if student is None:
            return {"ok": False, "error": "请先扫码添加账号"}
        lat = str(payload.get("lat", "")).strip()
        lng = str(payload.get("lng", "")).strip()
        if not lat or not lng:
            return {"ok": False, "error": "纬度和经度都要填"}
        student["lat"] = lat
        student["lng"] = lng
        student["acc"] = str(payload.get("acc", "30")).strip() or "30"
        student["WXKey"] = str(payload.get("wx", "")).strip()
        student["QQKey"] = str(payload.get("qq", "")).strip()
        if payload.get("name"):
            student["name"] = str(payload["name"]).strip()
        self._save_config()
        self.log.write(f"已保存账号 {student.get('name', '')} 的坐标与通知设置\n")
        return {"ok": True, "state": self.state()}

    def delete_student(self):
        if not (0 <= self.selected < len(self.students)):
            return {"ok": False, "error": "没有选中的账号"}
        removed = self.students.pop(self.selected)
        self.selected = min(self.selected, len(self.students) - 1)
        self._save_config()
        self.log.write(f"已删除账号 {removed.get('name', '')}\n")
        return {"ok": True, "state": self.state()}

    # ---------- 时间与设置 ----------

    def add_time(self, value):
        try:
            parsed = parse_times([value]) if value else []
        except (ValueError, TypeError):
            parsed = []
        if not parsed:
            return {"ok": False, "error": "时间格式应为 HH:MM 或 HH:MM:SS"}
        times = list(self.settings.get("times", []))
        if parsed[0] not in times:
            times.append(parsed[0])
            times.sort()
            self.settings["times"] = times
            self._save_settings()
        return {"ok": True, "state": self.state()}

    def remove_time(self, value):
        times = [t for t in self.settings.get("times", []) if t != value]
        self.settings["times"] = times
        self._save_settings()
        return {"ok": True, "state": self.state()}

    def save_settings(self, payload):
        payload = payload or {}
        raw = {
            "times": self.settings.get("times", []),
            "retry_attempts": payload.get("retry_attempts", self.settings.get("retry_attempts", 3)),
            "retry_delay_minutes": payload.get(
                "retry_delay_minutes", self.settings.get("retry_delay_minutes", 5)
            ),
        }
        self.settings = settings_store.normalize(raw)
        self._save_settings()
        if "autostart" in payload:
            autostart.set_autostart(bool(payload["autostart"]))
        return {"ok": True, "state": self.state()}

    # ---------- 调度 ----------

    def _on_event(self, name, payload):
        if name == "next_run" and payload.get("at"):
            self.next_run_at = payload["at"]
            self.status_text = f"运行中 · 下次 {self.next_run_at.strftime('%H:%M:%S')}"
        elif name == "running":
            self.status_text = "运行中 · 正在签到"
        elif name == "paused":
            self.status_text = "已暂停（到点不执行）"
        elif name == "stopped":
            self.status_text = "已停止"
        elif name == "result":
            code = payload.get("exit_code")
            self.status_text = "上次执行成功" if code == 0 else f"上次执行失败（退出码 {code}）"
        elif name == "retry":
            self.status_text = f"失败，{payload.get('delay', 0) // 60} 分钟后重试"

    def start(self):
        if self.scheduler and self.scheduler.running:
            return {"ok": True, "state": self.state()}
        if not self.settings.get("times"):
            return {"ok": False, "error": "请先添加至少一个执行时间"}
        self.scheduler = DailyScheduler(
            lambda: run_once(str(self.config_path)),
            lambda: list(self.settings.get("times", [])),
            retry_attempts=self.settings.get("retry_attempts", 3),
            retry_delay_seconds=self.settings.get("retry_delay_minutes", 5) * 60,
            on_event=self._on_event,
        )
        self.scheduler.start()
        self.status_text = "运行中 · 正在计算下次执行时间…"
        self.log.write(f"后台调度已启动，每天 {', '.join(self.settings.get('times', []))} 执行\n")
        return {"ok": True, "state": self.state()}

    def stop(self):
        if self.scheduler:
            self.scheduler.stop()
        self.status_text = "已停止"
        self.next_run_at = None
        return {"ok": True, "state": self.state()}

    def toggle_pause(self):
        if self.scheduler is None:
            return {"ok": False, "error": "还没有启动后台运行"}
        if self.scheduler.paused:
            self.scheduler.resume()
            self.status_text = "运行中 · 已恢复"
        else:
            self.scheduler.pause()
            self.status_text = "已暂停（到点不执行）"
        return {"ok": True, "state": self.state()}

    def run_now(self):
        def worker():
            self.status_text = "运行中 · 正在签到"
            try:
                code = run_once(str(self.config_path))
                self.status_text = "上次执行成功" if code == 0 else f"上次执行失败（退出码 {code}）"
            except Exception as exc:  # noqa: BLE001
                self.status_text = f"执行异常：{exc}"

        threading.Thread(target=worker, name="bjmf-run-now", daemon=True).start()
        return {"ok": True}

    # ---------- 系统 ----------

    def open_data_dir(self):
        paths.ensure_data_dir()
        import os

        os.startfile(str(paths.ensure_data_dir()))
        return {"ok": True}

    def open_amap(self):
        import webbrowser

        webbrowser.open("https://lbs.amap.com/tools/picker")
        return {"ok": True}

    # ---------- 扫码 ----------

    def qr_start(self):
        try:
            client = QRLoginClient()
            content = client.create()
            self._qr_client = client
            self._qr_state = "waiting"
            data_url = "data:image/png;base64," + base64.b64encode(content).decode()
            self.log.write("已生成登录二维码，请用微信扫码\n")
            return {"ok": True, "image": data_url}
        except Exception as exc:  # noqa: BLE001
            self._qr_state = "error"
            return {"ok": False, "error": f"获取二维码失败：{exc}"}

    def qr_poll(self):
        if self._qr_client is None:
            return {"state": "idle"}
        try:
            confirmed, url = self._qr_client.poll()
            if not confirmed:
                return {"state": "waiting"}
            cookie = self._qr_client.exchange_cookie(url)
            profile = self._qr_client.fetch_profile(cookie) or {}
            name = (profile.get("name") or "").strip()
            class_id = (profile.get("class") or "").strip()
            if not cookie:
                self._qr_state = "error"
                return {"state": "error", "error": "没有拿到登录凭证，请重新扫码"}
            self.students.append(
                {
                    "name": name or f"账号{len(self.students) + 1}",
                    "class": class_id,
                    "lat": "",
                    "lng": "",
                    "acc": "30",
                    "cookie": cookie,
                    "WXKey": "",
                    "QQKey": "",
                }
            )
            self.selected = len(self.students) - 1
            self._save_config()
            self._qr_state = "done"
            self._qr_client = None
            self.log.write(f"扫码成功：{name or '未知姓名'} / {class_id or '未知班级'}\n")
            return {"state": "done", "state_snapshot": self.state()}
        except Exception as exc:  # noqa: BLE001
            self._qr_state = "error"
            return {"state": "error", "error": str(exc)}
