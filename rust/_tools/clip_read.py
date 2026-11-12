"""读剪贴板里的文本（UTF-16），用来验证复制日志的中文有没有乱码。

用法：python rust/_tools/clip_read.py
"""

import ctypes
from ctypes import wintypes

u = ctypes.windll.user32
k32 = ctypes.windll.kernel32

# 不声明签名的话，64 位句柄会被截成 int32，GlobalLock 直接访问违规
u.OpenClipboard.argtypes = [wintypes.HWND]
u.OpenClipboard.restype = wintypes.BOOL
u.GetClipboardData.argtypes = [wintypes.UINT]
u.GetClipboardData.restype = wintypes.HANDLE
u.CloseClipboard.restype = wintypes.BOOL
k32.GlobalLock.argtypes = [wintypes.HANDLE]
k32.GlobalLock.restype = ctypes.c_void_p
k32.GlobalUnlock.argtypes = [wintypes.HANDLE]
k32.GlobalUnlock.restype = wintypes.BOOL


def read_clipboard():
    CF_UNICODETEXT = 13
    if u.OpenClipboard(None) == 0:
        return None
    try:
        handle = u.GetClipboardData(CF_UNICODETEXT)
        if not handle:
            return None
        ptr = k32.GlobalLock(handle)
        if not ptr:
            return None
        try:
            text = ctypes.wstring_at(ptr)
        finally:
            k32.GlobalUnlock(handle)
        return text
    finally:
        u.CloseClipboard()


if __name__ == "__main__":
    text = read_clipboard()
    if text is None:
        print("剪贴板里没有文本")
    else:
        lines = text.splitlines()
        print(f"长度 {len(text)}，共 {len(lines)} 行")
        for line in lines[:6]:
            print("  " + line)
