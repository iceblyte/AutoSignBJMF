"""QA 自动化：起进程、截图、点坐标、敲键、判断窗口可见性。

这台机器没装 Pillow / pyautogui，全部用 ctypes 直接调 win32，够用就好。

    python rust/_tools/qa.py launch <exe> [KEY=VAL ...]
    python rust/_tools/qa.py visible <标题片段>
    python rust/_tools/qa.py list
    python rust/_tools/qa.py click <标题片段> <x> <y>     # x/y 是客户区坐标
    python rust/_tools/qa.py dclick <标题片段> <x> <y>
    python rust/_tools/qa.py drag <标题片段> <x1> <y1> <x2> <y2>
    python rust/_tools/qa.py key <vk>                     # 发给当前前台窗口
    python rust/_tools/qa.py types <文本>
    python rust/_tools/qa.py shot <标题片段> <out.png>
    python rust/_tools/qa.py kill
"""

import ctypes
import os
import subprocess
import sys
import time
from ctypes import wintypes

u = ctypes.windll.user32
gdi = ctypes.windll.gdi32

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shot import find_window, grab, to_png, variety  # noqa: E402

MOUSEEVENTF_LEFTDOWN = 0x0002
MOUSEEVENTF_LEFTUP = 0x0004
SW_RESTORE = 9


def windows_matching(match):
    """枚举所有标题匹配的顶层窗口（含被最小化的 —— 最小化时
    GetWindowRect 是 -32000 附近，宽高只有 160x28，不能按尺寸过滤）。"""
    out = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        if not u.IsWindowVisible(hwnd):
            return True
        length = u.GetWindowTextLengthW(hwnd)
        if length:
            buf = ctypes.create_unicode_buffer(length + 1)
            u.GetWindowTextW(hwnd, buf, length + 1)
            if match in buf.value:
                rect = wintypes.RECT()
                u.GetWindowRect(hwnd, ctypes.byref(rect))
                out.append((hwnd, rect, buf.value))
        return True

    u.EnumWindows(visit, 0)
    return out


def pick(match, restore=True):
    found = windows_matching(match)
    if not found:
        raise SystemExit(f"找不到标题含「{match}」的可见窗口")
    hwnd, rect, title = found[0]
    if restore:
        bring_front(hwnd)
        rect = wintypes.RECT()
        u.GetWindowRect(hwnd, ctypes.byref(rect))
    return hwnd, rect, title


def client_origin(hwnd):
    pt = wintypes.POINT(0, 0)
    u.ClientToScreen(hwnd, ctypes.byref(pt))
    return pt.x, pt.y


def bring_front(hwnd):
    if u.IsIconic(hwnd):
        u.ShowWindow(hwnd, SW_RESTORE)
    u.SetForegroundWindow(hwnd)
    time.sleep(0.15)


def click_at(sx, sy, double=False):
    # 先挪到旁边再挪到目标：只 SetCursorPos 一次时，若上次位置就在目标点，
    # Windows 不会再发 WM_MOUSEMOVE，gpui 的 hit test 就停在旧的命中元素上。
    u.SetCursorPos(int(sx) + 3, int(sy) + 3)
    time.sleep(0.12)
    u.SetCursorPos(int(sx), int(sy))
    time.sleep(0.18)
    u.mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
    time.sleep(0.03)
    u.mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
    if double:
        time.sleep(0.05)
        u.mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
        time.sleep(0.03)
        u.mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)


def cmd_click(match, x, y, double=False):
    hwnd, _rect, title = pick(match)
    bring_front(hwnd)
    ox, oy = client_origin(hwnd)
    click_at(ox + int(x), oy + int(y), double)
    print(f"click {title} client({x},{y}) -> screen({ox + int(x)},{oy + int(y)})")


def cmd_drag(match, x1, y1, x2, y2):
    hwnd, _rect, title = pick(match)
    ox, oy = client_origin(hwnd)
    # 先挪两下：gpui 的命中判定依赖最后一次 MouseMove，直接按下会用到旧位置
    u.SetCursorPos(ox + int(x1) + 4, oy + int(y1) + 4)
    time.sleep(0.15)
    u.SetCursorPos(ox + int(x1), oy + int(y1))
    time.sleep(0.25)
    u.mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
    time.sleep(0.15)
    steps = 12
    for i in range(1, steps + 1):
        px = ox + int(x1) + (int(x2) - int(x1)) * i // steps
        py = oy + int(y1) + (int(y2) - int(y1)) * i // steps
        u.SetCursorPos(px, py)
        time.sleep(0.03)
    time.sleep(0.1)
    u.mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
    print(f"drag {title} ({x1},{y1}) -> ({x2},{y2})")


def cmd_types(text):
    # 只处理 ASCII：用 VkKeyScan + shift 状态敲键
    for ch in text:
        vk_scan = u.VkKeyScanW(ord(ch))
        vk = vk_scan & 0xFF
        shift = (vk_scan >> 8) & 0xFF
        if shift & 1:
            u.keybd_event(0x10, 0, 0, 0)
        u.keybd_event(vk, 0, 0, 0)
        time.sleep(0.01)
        u.keybd_event(vk, 0, 2, 0)
        if shift & 1:
            u.keybd_event(0x10, 0, 2, 0)
        time.sleep(0.02)
    print(f"types {text!r}")


def cmd_key(vk):
    u.keybd_event(int(vk, 0), 0, 0, 0)
    time.sleep(0.02)
    u.keybd_event(int(vk, 0), 0, 2, 0)
    print(f"key {vk}")


def cmd_shot(match, out, mode="auto"):
    hwnd, rect, title = pick(match)
    width, height, raw, ok = grab(hwnd, rect, "print" if mode == "auto" else mode)
    if mode == "auto" and variety(raw) < 5:
        width, height, raw, _ = grab(hwnd, rect, "screen")
    with open(out, "wb") as fh:
        fh.write(to_png(width, height, raw))
    print(f"shot {title} {width}x{height} -> {out} (variety={variety(raw)})")


def cmd_visible(match):
    found = windows_matching(match)
    if found:
        hwnd, rect, title = found[0]
        print(f"visible {title} @({rect.left},{rect.top}) {rect.right - rect.left}x{rect.bottom - rect.top}")
        return 0
    print(f"hidden (no window matching {match})")
    return 1


def cmd_list():
    seen = []
    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if u.IsWindowVisible(hwnd):
            length = u.GetWindowTextLengthW(hwnd)
            if length:
                buf = ctypes.create_unicode_buffer(length + 1)
                u.GetWindowTextW(hwnd, buf, length + 1)
                rect = wintypes.RECT()
                u.GetWindowRect(hwnd, ctypes.byref(rect))
                seen.append((pid.value, buf.value, rect.right - rect.left, rect.bottom - rect.top))
        return True
    u.EnumWindows(visit, 0)
    for pid, title, w, h in seen:
        print(f"pid={pid:<6} {w}x{h}  {title}")


def cmd_launch(exe, pairs):
    env = os.environ.copy()
    env["AUTOSIGN_NO_SINGLE"] = "1"
    for pair in pairs:
        key, _, value = pair.partition("=")
        env[key] = value
    log = open("rust/_tools/_qa-run.log", "wb")
    proc = subprocess.Popen([exe], env=env, stdout=log, stderr=log, cwd=os.path.dirname(exe))
    print(proc.pid)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    cmd = sys.argv[1]
    if cmd == "launch":
        cmd_launch(sys.argv[2], sys.argv[3:])
    elif cmd == "shot":
        cmd_shot(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else "auto")
    elif cmd == "click":
        cmd_click(sys.argv[2], sys.argv[3], sys.argv[4], len(sys.argv) > 5 and sys.argv[5] == "double")
    elif cmd == "dclick":
        cmd_click(sys.argv[2], sys.argv[3], sys.argv[4], True)
    elif cmd == "drag":
        cmd_drag(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6])
    elif cmd == "key":
        cmd_key(sys.argv[2])
    elif cmd == "types":
        cmd_types(sys.argv[2])
    elif cmd == "visible":
        return cmd_visible(sys.argv[2])
    elif cmd == "list":
        cmd_list()
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
