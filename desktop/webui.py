"""WebView 版桌面界面：用系统 WebView2 渲染与原型一致的 HTML。

pywebview 的 js_api 把前端调用转给 DesktopService，业务逻辑与 tkinter 版共用。
若 WebView2 不可用，main.py 会回退到 tkinter 版（desktop/app.py）。
"""

import base64
import sys
from pathlib import Path

from desktop.service import DesktopService


def _asset(name):
    base = getattr(sys, "_MEIPASS", None)
    if base:
        return Path(base) / "desktop" / "webui" / name
    return Path(__file__).resolve().parent / "webui" / name


def _logo_path():
    base = getattr(sys, "_MEIPASS", None)
    if base:
        return Path(base) / "desktop" / "assets" / "logo.png"
    return Path(__file__).resolve().parent / "assets" / "logo.png"


class Bridge:
    """暴露给 JavaScript 的接口，方法名与 index.html 里的 api.* 一一对应。"""

    def __init__(self, service):
        # 注意：属性一律下划线开头。pywebview 会遍历公开属性，
        # 若把 window 挂成公开属性，会顺着 window.native... 无限递归并卡死 UI 线程。
        self._service = service
        self._window = None

    # 前端启动时取一次 logo（避免依赖相对路径）
    def logo(self):
        try:
            data = base64.b64encode(_logo_path().read_bytes()).decode()
            return "data:image/png;base64," + data
        except Exception:
            return ""

    def state(self):
        return self._service.state()

    def tick(self):
        return self._service.tick()

    def select_student(self, index):
        return self._service.select_student(index)

    def save_student(self, payload):
        return self._service.save_student(payload)

    def add_time(self, value):
        return self._service.add_time(value)

    def remove_time(self, value):
        return self._service.remove_time(value)

    def save_settings(self, payload):
        return self._service.save_settings(payload)

    def start(self):
        return self._service.start()

    def stop(self):
        return self._service.stop()

    def run_now(self):
        return self._service.run_now()

    def open_data_dir(self):
        return self._service.open_data_dir()

    def open_amap(self):
        return self._service.open_amap()

    def qr_start(self):
        return self._service.qr_start()

    def qr_poll(self):
        return self._service.qr_poll()


def run(debug=False):
    import webview

    service = DesktopService()
    bridge = Bridge(service)

    window = webview.create_window(
        "班级魔方自动签到",
        url=str(_asset("index.html")),
        js_api=bridge,
        width=1120,
        height=780,
        min_size=(940, 660),
        background_color="#f5f5f7",
        x=40,
        y=40,
    )
    bridge._window = window

    try:
        from desktop.tray import TrayController

        tray = TrayController(
            lambda action: {
                "show": window.show,
                "run_now": service.run_now,
                "toggle_pause": service.toggle_pause,
                "quit": window.destroy,
            }[action]()
        )
        tray.start()
    except Exception:
        pass

    webview.start(debug=debug, gui="edgechromium")

    if service.scheduler is not None:
        service.scheduler.stop()
    return 0


if __name__ == "__main__":
    sys.exit(run())
