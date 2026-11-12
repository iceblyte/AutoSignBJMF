"""验证「点导入不卡死」：对话框开着时，主窗口必须仍然响应。

做法：
1. 用 AUTOSIGN_TEST_DIALOG=import 启动程序（2 秒后自动弹文件对话框）
2. 等对话框出现（枚举类名 #32770 的窗口）
3. 用 IsHungAppWindow 判断主窗口是否无响应
4. 关掉对话框与程序

判定：
    主窗口未卡死 + 存在文件对话框  -> PASS
    主窗口卡死（标题带「无响应」）  -> FAIL（就是这次要修的 bug）
"""

import ctypes
import subprocess
import sys
import time
from ctypes import wintypes

u = ctypes.windll.user32
user32 = ctypes.windll.user32

EXE = r"D:\Workplace\Dev\Mine\AutoSignBJMF\rust\target\x86_64-pc-windows-msvc\debug\AutoSignBJMF.exe"
TITLE = "班级魔方自动签到"


def windows_of(pid, cls=None, title_like=None):
    hits = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        owner = wintypes.DWORD()
        u.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value != pid:
            return True
        n = u.GetWindowTextLengthW(hwnd)
        buf = ctypes.create_unicode_buffer(n + 1)
        u.GetWindowTextW(hwnd, buf, n + 1)
        name = ctypes.create_unicode_buffer(256)
        u.GetClassNameW(hwnd, name, 256)
        if cls is not None and name.value != cls:
            return True
        if title_like is not None and title_like not in buf.value:
            return True
        hits.append((hwnd, buf.value, name.value))
        return True

    u.EnumWindows(visit, 0)
    return hits


def main():
    subprocess.run(["taskkill", "/IM", "AutoSignBJMF.exe", "/F"], capture_output=True)
    time.sleep(1)

    env = {
        "AUTOSIGN_DATA_DIR": r"D:\Workplace\Dev\Mine\AutoSignBJMF\rust\app\demo",
        "AUTOSIGN_NO_TRAY": "1",
        "AUTOSIGN_OPEN": "settings",
        "AUTOSIGN_TEST_DIALOG": "import",
    }
    import os

    full_env = dict(os.environ)
    full_env.update(env)
    proc = subprocess.Popen([EXE], env=full_env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    # 等主窗口
    main_hwnd = None
    for _ in range(150):
        found = windows_of(proc.pid, title_like=TITLE)
        if found:
            main_hwnd = found[0][0]
            break
        time.sleep(0.1)
    if main_hwnd is None:
        print("FAIL: 主窗口没出现")
        proc.kill()
        return 1
    print(f"主窗口 hwnd={main_hwnd}")

    # 等文件对话框（系统对话框类名 #32770）
    dialog = None
    for _ in range(200):
        hits = windows_of(proc.pid, cls="#32770")
        if hits:
            dialog = hits[0]
            break
        time.sleep(0.1)

    if dialog is None:
        print("FAIL: 文件对话框没有出现")
        proc.kill()
        return 1
    print(f"文件对话框 hwnd={dialog[0]} 标题='{dialog[1]}'")

    # 关键断言：对话框开着的时候主窗口不能卡死
    time.sleep(1.5)
    hung = user32.IsHungAppWindow(main_hwnd)
    title_now = windows_of(proc.pid, title_like="")
    main_title = next((t for h, t, _ in title_now if h == main_hwnd), "")
    print(f"IsHungAppWindow={hung}  主窗口标题={main_title!r}")

    # 关掉对话框：给 OK/Cancel 发 ESC
    u.PostMessageW(dialog[0], 0x0100, 0x1B, 0)  # WM_KEYDOWN ESC
    time.sleep(1.0)

    still_alive = proc.poll() is None
    hung_after = user32.IsHungAppWindow(main_hwnd)
    print(f"关闭对话框后：进程存活={still_alive} IsHungAppWindow={hung_after}")

    proc.kill()

    ok = (hung == 0) and still_alive and (hung_after == 0)
    print("PASS: 对话框打开期间主窗口保持响应" if ok else "FAIL: 主窗口卡死或进程异常")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
