"""从 Windows SDK 的 layout 目录里取出 fxc.exe（只取，不安装 SDK）。

背景：SDK 的 MSI 把真实文件名藏在 File 表里，cab 内的条目名是哈希（filXXXX），
所以不能靠搜文件名，必须读 MSI 的 File / Media 两张表建立映射。

依赖：pip install pymsi；7z 在 PATH 里。

用法：
    python find_fxc.py <layout-installers 目录> <输出目录>
"""

import pathlib
import shutil
import struct
import subprocess
import sys

from pymsi import Package

MACHINE = {0x8664: "x64", 0x014C: "x86", 0xAA64: "arm64"}


def find_rows(installers: pathlib.Path):
    """遍历所有 MSI，返回 [(msi, 内部键, cab 名)]，只针对 fxc.exe。"""
    out = []
    for msi in sorted(installers.glob("*.msi")):
        try:
            pkg = Package(msi, strict=False)
            files = pkg.get("File")
            media = pkg.get("Media")
        except Exception:
            continue

        media_rows = sorted(
            (m.get("LastSequence"), m.get("Cabinet")) for m in media.rows
        )

        for row in files.rows:
            if str(row.get("FileName", "")).lower() != "fxc.exe":
                continue
            seq = row.get("Sequence")
            cab = next((c for last, c in media_rows if last >= seq), None)
            if cab:
                out.append((msi, row.get("File"), cab))
    return out


def machine_of(path: pathlib.Path) -> str:
    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    return MACHINE.get(struct.unpack_from("<H", data, pe + 4)[0], "unknown")


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 1

    installers = pathlib.Path(sys.argv[1])
    outdir = pathlib.Path(sys.argv[2])
    outdir.mkdir(parents=True, exist_ok=True)
    tmp = outdir / "_tmp"
    tmp.mkdir(exist_ok=True)

    rows = find_rows(installers)
    if not rows:
        print("没有在任何 MSI 里找到 fxc.exe")
        return 2

    for msi, key, cab in rows:
        subprocess.run(
            ["7z", "e", str(installers / cab), key, f"-o{tmp}", "-y"],
            check=True,
            capture_output=True,
        )
        got = tmp / key
        if not got.exists():
            continue
        arch = machine_of(got)
        target = outdir / f"fxc-{arch}.exe"
        shutil.copy(got, target)
        print(f"{msi.name} · {cab} -> {target.name} ({got.stat().st_size} bytes)")

    # x64 的那份作为默认
    x64 = outdir / "fxc-x64.exe"
    if x64.exists():
        shutil.copy(x64, outdir / "fxc.exe")
        print(f"\n已就绪：{outdir / 'fxc.exe'}")
        print(f'    export GPUI_FXC_PATH="{outdir / "fxc.exe"}"')

    shutil.rmtree(tmp, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
