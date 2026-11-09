"""生成 exe 图标（build/app.ico），避免仓库里放二进制还要手工维护。"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from desktop.tray import _make_icon_image  # noqa: E402


def main():
    target = Path(__file__).resolve().parent / "app.ico"
    image = _make_icon_image(256)
    image.save(target, format="ICO", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print(f"icon written: {target}")


if __name__ == "__main__":
    main()
