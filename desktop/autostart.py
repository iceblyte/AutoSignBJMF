"""开机自启：写当前用户的 Run 注册表项（不需要管理员权限）。"""

import sys
from pathlib import Path

RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
VALUE_NAME = "AutoSignBJMF"


def _launch_command():
    if getattr(sys, "frozen", False):
        return f'"{sys.executable}"'
    entry = Path(__file__).resolve().parent.parent / "desktop" / "main.py"
    return f'"{sys.executable}" "{entry}"'


def supported():
    return sys.platform == "win32"


def set_autostart(enabled):
    """开启/关闭开机自启；返回是否成功。"""
    if not supported():
        return False
    import winreg

    try:
        with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
            if enabled:
                winreg.SetValueEx(key, VALUE_NAME, 0, winreg.REG_SZ, _launch_command())
            else:
                try:
                    winreg.DeleteValue(key, VALUE_NAME)
                except FileNotFoundError:
                    pass
    except OSError:
        return False
    return True


def is_autostart_enabled():
    if not supported():
        return False
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_READ) as key:
            winreg.QueryValueEx(key, VALUE_NAME)
            return True
    except OSError:
        return False
