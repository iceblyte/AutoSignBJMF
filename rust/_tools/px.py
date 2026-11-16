"""读 shot.py 产出的 PNG（filter 0），做两件事：

    python rust/_tools/px.py <png> rows <y1> <y2> <hexcolor>   # 每行该色的连续区间
    python rust/_tools/px.py <png> crop <x> <y> <w> <h> <scale> <out.png>
    python rust/_tools/px.py <png> at <x> <y>                  # 该点颜色
"""

import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos = 8
    idat = b""
    width = height = 0
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        tag = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if tag == b"IHDR":
            width, height = struct.unpack(">II", body[:8])
        elif tag == b"IDAT":
            idat += body
        pos += 12 + length
    raw = zlib.decompress(idat)
    stride = width * 4
    rows = []
    for y in range(height):
        line = raw[y * (stride + 1) : (y + 1) * (stride + 1)]
        assert line[0] == 0, "只支持 filter 0"
        rows.append(line[1:])
    return width, height, rows


def write_png(path, width, height, rows):
    raw = b"".join(b"\x00" + r for r in rows)
    comp = zlib.compress(raw, 6)

    def chunk(tag, body):
        return (
            struct.pack(">I", len(body))
            + tag
            + body
            + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    open(path, "wb").write(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", comp) + chunk(b"IEND", b"")
    )


def rgb_at(rows, x, y):
    line = rows[y]
    r, g, b = line[x * 4], line[x * 4 + 1], line[x * 4 + 2]
    return (r << 16) | (g << 8) | b


def main():
    path = sys.argv[1]
    width, height, rows = read_png(path)
    cmd = sys.argv[2]
    if cmd == "rows":
        y1, y2, color = int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5], 16)
        for y in range(y1, min(y2, height)):
            runs = []
            start = None
            for x in range(width):
                hit = rgb_at(rows, x, y) == color
                if hit and start is None:
                    start = x
                elif not hit and start is not None:
                    runs.append((start, x - 1))
                    start = None
            if start is not None:
                runs.append((start, width - 1))
            if runs:
                print(y, runs)
    elif cmd == "at":
        print(hex(rgb_at(rows, int(sys.argv[3]), int(sys.argv[4]))))
    elif cmd == "crop":
        x0, y0, w, h, scale, out = (
            int(sys.argv[3]),
            int(sys.argv[4]),
            int(sys.argv[5]),
            int(sys.argv[6]),
            int(sys.argv[7]),
            sys.argv[8],
        )
        cropped = []
        for y in range(y0, min(y0 + h, height)):
            line = rows[y]
            out_line = bytearray()
            for x in range(x0, min(x0 + w, width)):
                px = line[x * 4 : x * 4 + 4]
                out_line += px * scale
            for _ in range(scale):
                cropped.append(bytes(out_line))
        write_png(out, min(w, width - x0) * scale, len(cropped), cropped)
        print(f"crop -> {out}")
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
