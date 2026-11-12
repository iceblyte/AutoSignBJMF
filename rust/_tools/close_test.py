"""验证「关窗 → 隐藏到托盘、进程存活」这条交互。

自己拉起 exe、自己发 WM_CLOSE，全程不经 PowerShell。
"""

import ctypes
import subprocess
import sys
import time
from ctypes import wintypes

u = ctypes.windll.user32
EXE = r"D:\Workplace\Dev\Mine\AutoSignBJMF\rust\target\debug\AutoSignBJMF.exe"


def windows_of(pid):
    found = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def cb(hwnd, _):
        owner = wintypes.DWORD()
        u.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid:
            rect = wintypes.RECT()
            u.GetWindowRect(hwnd, ctypes.byref(rect))
            if rect.right - rect.left > 400:
                found.append((hwnd, bool(u.IsWindowVisible(hwnd))))
        return True

    u.EnumWindows(cb, 0)
    return found


def main():
    proc = subprocess.Popen(
        [EXE],
        env={
            "AUTOSIGN_DATA_DIR": r"D:\Workplace\Dev\Mine\AutoSignBJMF\rust\app\demo",
            "AUTOSIGN_NO_TRAY": "1",
            "SYSTEMROOT": r"C:\Windows",
            "PATH": r"C:\Windows\System32",
        },
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(8)

    found = windows_of(proc.pid)
    print("启动后窗口:", [(h, v) for h, v in found])
    if not found:
        print("FAIL: 没有找到窗口")
        proc.kill()
        return 1

    hwnd = found[0][0]
    u.PostMessageW(hwnd, 0x0010, 0, 0)  # WM_CLOSE
    time.sleep(3)

    after = windows_of(proc.pid)
    visible = [v for _, v in after]
    alive = proc.poll() is None
    print("关闭后窗口:", [(h, v) for h, v in after])
    print("进程存活:", alive)

    proc.kill()
    if alive and visible and not any(visible):
        print("PASS: 窗口已隐藏，进程仍在运行")
        return 0
    print("FAIL: 期望窗口隐藏且进程存活")
    return 1


if __name__ == "__main__":
    sys.exit(main())
