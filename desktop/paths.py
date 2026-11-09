"""路径解析：绿色版（exe 同级有 data/）优先，否则落到用户目录。

- 绿色版：`AutoSignBJMF.exe` 旁边有 `data/` → 就地读写，U 盘可带走
- 单 exe：没有 `data/` → 用 `%USERPROFILE%\\AutoSignBJMF\\`
"""

import os
import sys
from pathlib import Path

APP_NAME = "AutoSignBJMF"


def is_frozen():
    """是否运行在 PyInstaller 打出来的 exe 里。"""
    return bool(getattr(sys, "frozen", False))


def exe_dir():
    """exe 所在目录；源码运行时是项目根目录。"""
    if is_frozen():
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent.parent


def home_data_dir():
    """用户目录下的数据根：C:\\Users\\<name>\\AutoSignBJMF。"""
    return Path(os.path.expanduser("~")) / APP_NAME


def data_dir():
    """决定数据目录落在哪儿：绿色版优先，其次用户目录。"""
    portable = exe_dir() / "data"
    if portable.is_dir():
        return portable
    return home_data_dir()


def ensure_data_dir():
    """确保数据目录与 logs 子目录存在，并把老版本的 data.json 迁移进来。"""
    target = data_dir()
    (target / "logs").mkdir(parents=True, exist_ok=True)
    _migrate_legacy_config(target)
    return target


def _migrate_legacy_config(target):
    """把 exe 旁边遗留的 data.json 复制到数据目录，避免升级后配置丢失。"""
    destination = target / "data.json"
    if destination.exists():
        return

    candidates = [exe_dir() / "data.json", exe_dir() / "data" / "data.json"]
    for candidate in candidates:
        try:
            if candidate.is_file() and candidate.resolve() != destination.resolve():
                destination.write_bytes(candidate.read_bytes())
                return
        except OSError:
            # 迁移失败不影响启动，用户可以在界面里重新扫码
            continue


def config_path():
    return ensure_data_dir() / "data.json"


def settings_path():
    return ensure_data_dir() / "settings.json"


def log_dir():
    return ensure_data_dir() / "logs"
