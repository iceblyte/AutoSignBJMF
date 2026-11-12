"""不依赖 Pillow 的窗口截图：ctypes 调 GDI + 自己写 PNG。

这台机器上 Python 没装 Pillow，而验证 UI 又必须看图，所以把最小实现放这里。

用法：
    python rust/_tools/shot.py <窗口标题片段> <输出.png> [--mode print|screen]

    print  —— PrintWindow(PW_RENDERFULLCONTENT)，窗口被遮挡也能抓到
    screen —— 从屏幕 BitBlt 目标区域，需要窗口在最前面

默认先试 print，结果若整片同色再自动退回 screen。
"""

import ctypes
import struct
import sys
import zlib
from ctypes import wintypes

u = ctypes.windll.user32
gdi = ctypes.windll.gdi32


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wintypes.DWORD * 3)]


def find_window(match):
    found = []

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
                if rect.right - rect.left > 200:
                    found.append((hwnd, rect, buf.value))
        return True

    u.EnumWindows(visit, 0)
    return found


def grab(hwnd, rect, mode):
    width = rect.right - rect.left
    height = rect.bottom - rect.top
    hdc = u.GetWindowDC(hwnd)
    memdc = gdi.CreateCompatibleDC(hdc)
    bmp = gdi.CreateCompatibleBitmap(hdc, width, height)
    gdi.SelectObject(memdc, bmp)

    if mode == "print":
        # PW_RENDERFULLCONTENT = 2
        ok = u.PrintWindow(hwnd, memdc, 2)
    else:
        screen = u.GetDC(0)
        ok = gdi.BitBlt(
            memdc, 0, 0, width, height, screen, rect.left, rect.top, 0x00CC0020
        )
        u.ReleaseDC(0, screen)

    info = BITMAPINFO()
    info.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    info.bmiHeader.biWidth = width
    info.bmiHeader.biHeight = -height  # 负数 = 顶行在前
    info.bmiHeader.biPlanes = 1
    info.bmiHeader.biBitCount = 32
    info.bmiHeader.biCompression = 0
    buffer = ctypes.create_string_buffer(width * height * 4)
    gdi.GetDIBits(memdc, bmp, 0, height, buffer, ctypes.byref(info), 0)

    gdi.DeleteObject(bmp)
    gdi.DeleteDC(memdc)
    u.ReleaseDC(hwnd, hdc)
    return width, height, buffer.raw, ok


def to_png(width, height, raw):
    """BGRA -> PNG（RGBA，filter 0）。"""
    rows = []
    for y in range(height):
        line = raw[y * width * 4 : (y + 1) * width * 4]
        out = bytearray()
        for x in range(0, len(line), 4):
            out += bytes((line[x + 2], line[x + 1], line[x], line[x + 3]))
        rows.append(b"\x00" + bytes(out))
    compressed = zlib.compress(b"".join(rows), 6)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", compressed)
        + chunk(b"IEND", b"")
    )


def variety(raw):
    """粗略判断画面是否「有内容」：统计不同颜色的数量。"""
    seen = set()
    for i in range(0, len(raw), 4 * 97):  # 稀疏采样
        seen.add(raw[i : i + 3])
        if len(seen) > 40:
            break
    return len(seen)


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 1
    match, out = sys.argv[1], sys.argv[2]
    mode = "auto"
    if "--mode" in sys.argv:
        mode = sys.argv[sys.argv.index("--mode") + 1]

    found = find_window(match)
    if not found:
        print(f"找不到标题含「{match}」的可见窗口")
        return 2
    hwnd, rect, title = found[0]

    if mode in ("auto", "print"):
        width, height, raw, ok = grab(hwnd, rect, "print")
        if variety(raw) < 5:  # 基本是纯色，说明 PrintWindow 抓不到
            if mode == "auto":
                print("PrintWindow 结果疑似空白，改用屏幕抓取")
                width, height, raw, ok = grab(hwnd, rect, "screen")
            else:
                print("注意：画面颜色种类很少，可能是空白")
    else:
        width, height, raw, ok = grab(hwnd, rect, "screen")

    data = to_png(width, height, raw)
    with open(out, "wb") as fh:
        fh.write(data)
    print(f"窗口='{title}' {width}x{height} @({rect.left},{rect.top}) -> {out}")
    print(f"颜色种类≈{variety(raw)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
