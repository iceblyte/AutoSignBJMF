"""系统托盘图标。

pystray 在独立线程里跑，所有菜单回调只往界面队列里丢动作，
不直接碰 tkinter（Tk 不是线程安全的）。
"""

import threading

APP_NAME = "班级魔方自动签到"


def _make_icon_image(size=64):
    """画一个蓝底对勾的图标，省得额外带 .ico 资源。"""
    from PIL import Image, ImageDraw

    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((3, 3, size - 3, size - 3), radius=14, fill=(37, 99, 235, 255))
    width = max(3, size // 10)
    draw.line((size * 0.26, size * 0.52, size * 0.44, size * 0.70), fill="white", width=width)
    draw.line((size * 0.44, size * 0.70, size * 0.75, size * 0.33), fill="white", width=width)
    return image


class TrayController:
    def __init__(self, dispatch, app_name=APP_NAME):
        """dispatch(action) 由界面提供，把动作塞进主线程队列。"""
        self._dispatch = dispatch
        self._app_name = app_name
        self._icon = None

    @property
    def available(self):
        return self._icon is not None

    def _menu(self):
        import pystray

        return pystray.Menu(
            pystray.MenuItem("显示主窗口", lambda: self._dispatch("show"), default=True),
            pystray.MenuItem("立即签到一次", lambda: self._dispatch("run_now")),
            pystray.MenuItem("暂停 / 恢复调度", lambda: self._dispatch("toggle_pause")),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("退出", lambda: self._dispatch("quit")),
        )

    def start(self):
        try:
            import pystray
        except Exception:
            return False

        try:
            self._icon = pystray.Icon("bjmf", _make_icon_image(), self._app_name, menu=self._menu())
            threading.Thread(target=self._icon.run, name="bjmf-tray", daemon=True).start()
        except Exception:
            self._icon = None
            return False
        return True

    def stop(self):
        icon, self._icon = self._icon, None
        if icon is not None:
            try:
                icon.stop()
            except Exception:
                pass

    def notify(self, message, title=None):
        """弹 Windows 气泡提示；失败就算了，不影响主流程。"""
        if self._icon is None:
            return
        try:
            self._icon.notify(message, title or self._app_name)
        except Exception:
            pass
