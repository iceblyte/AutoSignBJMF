"""把屏幕右下角通知区放大 n 倍导出，用来肉眼确认托盘图标。

用法：python rust/_tools/tray_zoom.py <x> <y> <w> <h> <scale> <out.png>
坐标是相对屏幕左上角的像素。
"""

import ctypes
import struct
import sys
import zlib
from ctypes import wintypes

u = ctypes.windll.user32
gdi = ctypes.windll.gdi32


class BIH(ctypes.Structure):
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


class BI(ctypes.Structure):
    _fields_ = [("bmiHeader", BIH), ("bmiColors", wintypes.DWORD * 3)]


def grab(x, y, w, h):
    screen = u.GetDC(0)
    memdc = gdi.CreateCompatibleDC(screen)
    bmp = gdi.CreateCompatibleBitmap(screen, w, h)
    gdi.SelectObject(memdc, bmp)
    gdi.BitBlt(memdc, 0, 0, w, h, screen, x, y, 0x00CC0020)

    info = BI()
    info.bmiHeader.biSize = ctypes.sizeof(BIH)
    info.bmiHeader.biWidth = w
    info.bmiHeader.biHeight = -h
    info.bmiHeader.biPlanes = 1
    info.bmiHeader.biBitCount = 32
    info.bmiHeader.biCompression = 0
    buf = ctypes.create_string_buffer(w * h * 4)
    gdi.GetDIBits(memdc, bmp, 0, h, buf, ctypes.byref(info), 0)

    gdi.DeleteObject(bmp)
    gdi.DeleteDC(memdc)
    u.ReleaseDC(0, screen)
    return buf.raw


def write_png(path, w, h, raw):
    rows = []
    for y in range(h):
        line = raw[y * w * 4 : (y + 1) * w * 4]
        out = bytearray()
        for i in range(0, len(line), 4):
            out += bytes((line[i + 2], line[i + 1], line[i]))
        rows.append(b"\x00" + bytes(out))
    data = zlib.compress(b"".join(rows), 6)

    def chunk(tag, body):
        return (
            struct.pack(">I", len(body))
            + tag
            + body
            + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
        )

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", data)
        + chunk(b"IEND", b"")
    )
    open(path, "wb").write(png)


def main():
    x, y, w, h, scale = (int(v) for v in sys.argv[1:6])
    out = sys.argv[6]
    raw = grab(x, y, w, h)
    # 最近邻放大
    W, H = w * scale, h * scale
    big = bytearray(W * H * 4)
    for ty in range(H):
        for tx in range(W):
            src = ((ty // scale) * w + (tx // scale)) * 4
            dst = (ty * W + tx) * 4
            big[dst : dst + 4] = raw[src : src + 4]
    write_png(out, W, H, bytes(big))
    print(f"截取 ({x},{y}) {w}x{h} -> {out} ({W}x{H})")


if __name__ == "__main__":
    main()
