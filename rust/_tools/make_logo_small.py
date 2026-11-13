"""把 logo.png 预缩成小尺寸 PNG，供界面内直接贴图。

原图 512×512，标题栏只显示 16px，直接交给渲染器缩 32 倍，边缘会糊成一团。
先在离线阶段用 alpha 加权的面积平均缩到 64×64（按 2× 超采样显示 32px），
界面里再缩小就只有 2:1，形状能保住。

用法：python rust/_tools/make_logo_small.py
输出：rust/app/assets/logo-64.png
"""

import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from make_tray_icon import alpha_bbox, read_png  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "design", "v2", "assets", "logo.png")
DST = os.path.join(ROOT, "rust", "app", "assets", "logo-64.png")
SIZE = 64
PAD = 0.02  # 几乎贴边，界面里由容器留白


def resize_rgba(w, h, rgba, box, out_size, pad):
    left, top, right, bottom = box
    inner = out_size * (1.0 - pad * 2)
    scale = inner / max(right - left, bottom - top)
    offset = (out_size - (right - left) * scale) / 2.0
    offset_y = (out_size - (bottom - top) * scale) / 2.0

    out = bytearray(out_size * out_size * 4)
    for oy in range(out_size):
        for ox in range(out_size):
            sx0 = left + (ox - offset) / scale
            sy0 = top + (oy - offset_y) / scale
            sx1 = sx0 + 1.0 / scale
            sy1 = sy0 + 1.0 / scale

            x0, x1 = int(max(sx0, left)), int(min(sx1, right))
            y0, y1 = int(max(sy0, top)), int(min(sy1, bottom))
            if x1 <= x0 or y1 <= y0:
                continue

            # alpha 加权平均，避免透明边缘混进黑边
            r = g = b = a = 0.0
            count = 0
            for py in range(y0, y1):
                for px in range(x0, x1):
                    o = (py * w + px) * 4
                    pa = rgba[o + 3] / 255.0
                    r += rgba[o] * pa
                    g += rgba[o + 1] * pa
                    b += rgba[o + 2] * pa
                    a += pa
                    count += 1
            if count == 0 or a <= 0:
                continue
            o = (oy * out_size + ox) * 4
            out[o] = int(min(255, r / a))
            out[o + 1] = int(min(255, g / a))
            out[o + 2] = int(min(255, b / a))
            out[o + 3] = int(min(255, (a / count) * 255.0))  # a 是 0–1 的平均值
    return bytes(out)


def encode_png(size, rgba):
    rows = []
    for y in range(size):
        line = rgba[y * size * 4 : (y + 1) * size * 4]
        rows.append(b"\x00" + bytes(line))
    data = zlib.compress(b"".join(rows), 9)

    def chunk(tag, body):
        return (
            struct.pack(">I", len(body))
            + tag
            + body
            + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", data)
        + chunk(b"IEND", b"")
    )


def main():
    w, h, rgba = read_png(SRC)
    box = alpha_bbox(w, h, rgba)
    small = resize_rgba(w, h, rgba, box, SIZE, PAD)
    with open(DST, "wb") as fh:
        fh.write(encode_png(SIZE, small))
    print(f"源 {w}x{h} 裁切 {box} -> {DST}（{SIZE}x{SIZE}, {os.path.getsize(DST)} 字节）")


if __name__ == "__main__":
    main()
