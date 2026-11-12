"""把 design/v2/assets/logo.png 转成托盘用的 32x32 RGBA 原始字节。

不依赖 Pillow：自己解 PNG（8bit、非隔行、RGB/RGBA/调色板都支持），
用 alpha 预乘的盒式平均缩放，避免缩小后边缘发黑。

用法：
    python rust/_tools/make_tray_icon.py
输出：
    rust/app/assets/tray-32.rgba   （32*32*4 = 4096 字节，RGBA 顺序）
"""

import io
import os
import struct
import sys
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "design", "v2", "assets", "logo.png")
DST = os.path.join(ROOT, "rust", "app", "assets", "tray-32.rgba")
SIZE = 32
PAD = 0.06  # logo 贴边时留一点空隙，否则托盘里会显得顶格


def read_png(path):
    data = io.open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "不是 PNG"
    pos = 8
    width = height = depth = color = 0
    idat = bytearray()
    palette = b""
    trns = b""
    while pos < len(data):
        (length,) = struct.unpack_from(">I", data, pos)
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, color, comp, filt, interlace = struct.unpack(">IIBBBBB", body)
            assert depth == 8, f"只支持 8bit，实际 {depth}"
            assert interlace == 0, "不支持隔行 PNG"
        elif kind == b"PLTE":
            palette = body
        elif kind == b"tRNS":
            trns = body
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color]
    stride = width * channels
    out = bytearray(height * stride)
    prev = bytearray(stride)
    p = 0
    for y in range(height):
        ftype = raw[p]
        p += 1
        line = bytearray(raw[p : p + stride])
        p += stride
        if ftype == 1:  # Sub
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif ftype == 2:  # Up
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ftype == 3:  # Average
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif ftype == 4:  # Paeth
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = prev[i]
                c = prev[i - channels] if i >= channels else 0
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        out[y * stride : (y + 1) * stride] = line
        prev = line

    # 统一转成 RGBA
    rgba = bytearray(width * height * 4)
    for i in range(width * height):
        if color == 6:
            rgba[i * 4 : i * 4 + 4] = out[i * 4 : i * 4 + 4]
        elif color == 2:
            r, g, b = out[i * 3 : i * 3 + 3]
            rgba[i * 4 : i * 4 + 4] = bytes((r, g, b, 255))
        elif color == 0:
            v = out[i]
            rgba[i * 4 : i * 4 + 4] = bytes((v, v, v, 255))
        elif color == 4:
            v, a = out[i * 2 : i * 2 + 2]
            rgba[i * 4 : i * 4 + 4] = bytes((v, v, v, a))
        elif color == 3:
            idx = out[i]
            r, g, b = palette[idx * 3 : idx * 3 + 3]
            a = trns[idx] if idx < len(trns) else 255
            rgba[i * 4 : i * 4 + 4] = bytes((r, g, b, a))
    return width, height, rgba


def alpha_bbox(width, height, rgba):
    """非透明像素的包围盒：logo 四周可能有大片透明，先裁掉再缩放。"""
    min_x, min_y, max_x, max_y = width, height, -1, -1
    for y in range(height):
        for x in range(width):
            if rgba[(y * width + x) * 4 + 3] > 8:
                min_x = min(min_x, x)
                max_x = max(max_x, x)
                min_y = min(min_y, y)
                max_y = max(max_y, y)
    if max_x < 0:
        return 0, 0, width, height
    return min_x, min_y, max_x + 1, max_y + 1


def resize(width, height, rgba, box, out_size, pad):
    x0, y0, x1, y1 = box
    bw, bh = x1 - x0, y1 - y0
    inner = max(1, int(round(out_size * (1.0 - pad * 2))))
    offset = (out_size - inner) // 2
    out = bytearray(out_size * out_size * 4)

    for ty in range(inner):
        for tx in range(inner):
            # 目标像素对应源图的范围
            sx0 = x0 + bw * tx / inner
            sx1 = x0 + bw * (tx + 1) / inner
            sy0 = y0 + bh * ty / inner
            sy1 = y0 + bh * (ty + 1) / inner
            iy0, iy1 = int(sy0), max(int(sy1), int(sy0) + 1)
            ix0, ix1 = int(sx0), max(int(sx1), int(sx0) + 1)

            rsum = gsum = bsum = asum = 0.0
            count = 0
            for sy in range(iy0, min(iy1, y1)):
                for sx in range(ix0, min(ix1, x1)):
                    o = (sy * width + sx) * 4
                    a = rgba[o + 3] / 255.0
                    # 预乘，避免半透明像素把颜色拉黑
                    rsum += rgba[o] * a
                    gsum += rgba[o + 1] * a
                    bsum += rgba[o + 2] * a
                    asum += a
                    count += 1
            if count == 0:
                continue
            alpha = asum / count
            o = ((ty + offset) * out_size + (tx + offset)) * 4
            if alpha > 0:
                out[o] = int(round(rsum / asum))
                out[o + 1] = int(round(gsum / asum))
                out[o + 2] = int(round(bsum / asum))
            out[o + 3] = int(round(alpha * 255))
    return out


def main():
    width, height, rgba = read_png(SRC)
    box = alpha_bbox(width, height, rgba)
    out = resize(width, height, rgba, box, SIZE, PAD)
    with open(DST, "wb") as fh:
        fh.write(out)

    total = SIZE * SIZE
    transparent = sum(1 for i in range(total) if out[i * 4 + 3] == 0)
    center = ((SIZE // 2) * SIZE + SIZE // 2) * 4
    print(f"源图 {width}x{height}，裁切框 {box}")
    print(f"写出 {DST}（{len(out)} 字节）")
    print(f"完全透明像素 {transparent}/{total}")
    print(f"中心像素 RGBA = {tuple(out[center:center+4])}")


if __name__ == "__main__":
    sys.exit(main())
