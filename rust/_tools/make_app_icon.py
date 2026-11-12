"""生成 app.ico：从 design/v2/assets/logo.png 出多尺寸图标。

之前那版留白 22%，任务栏和 exe 图标看起来比系统图标小一圈；这里改成 6%，
并补齐 Windows 常用尺寸（16/20/24/32/40/48/64/128/256）。

ICO 里每个尺寸存 PNG（Vista 起支持），这样不用手写 BMP/AND 掩码。

用法：python rust/_tools/make_app_icon.py
输出：rust/app/assets/app.ico
"""

import io
import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from make_tray_icon import alpha_bbox, read_png, resize  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "design", "v2", "assets", "logo.png")
DST = os.path.join(ROOT, "rust", "app", "assets", "app.ico")
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
PAD = 0.06


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
    width, height, rgba = read_png(SRC)
    box = alpha_bbox(width, height, rgba)

    images = []
    for size in SIZES:
        scaled = resize(width, height, rgba, box, size, PAD)
        images.append((size, encode_png(size, scaled)))

    header = struct.pack("<HHH", 0, 1, len(images))
    entry_size = 16
    offset = len(header) + entry_size * len(images)

    entries = b""
    payload = b""
    for size, data in images:
        entries += struct.pack(
            "<BBBBHHII",
            size if size < 256 else 0,  # 256 要写成 0
            size if size < 256 else 0,
            0,
            0,
            1,
            32,
            len(data),
            offset,
        )
        offset += len(data)
        payload += data

    with open(DST, "wb") as fh:
        fh.write(header + entries + payload)

    print(f"源图 {width}x{height}，裁切框 {box}，留白 {PAD:.0%}")
    print(f"写出 {DST}（{os.path.getsize(DST)} 字节，{len(images)} 个尺寸）")
    print("尺寸：", ", ".join(str(s) for s in SIZES))


if __name__ == "__main__":
    main()
