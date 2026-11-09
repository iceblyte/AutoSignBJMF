"""桌面端设置：每天的时间点、失败重试、开机自启等。

存放在数据目录的 settings.json；读坏了或字段非法时回退到默认值，不让程序起不来。
"""

import json

from utils.timepoints import parse_times

DEFAULT_TIMES = ["21:31:00"]
DEFAULT_RETRY_ATTEMPTS = 3
DEFAULT_RETRY_DELAY_MINUTES = 5
MAX_RETRY_ATTEMPTS = 10
MAX_RETRY_DELAY_MINUTES = 120


def default_settings():
    return {
        "times": list(DEFAULT_TIMES),
        "retry_attempts": DEFAULT_RETRY_ATTEMPTS,
        "retry_delay_minutes": DEFAULT_RETRY_DELAY_MINUTES,
        "autostart": False,
        "minimize_to_tray": True,
        "acc": "30",
    }


def _clamp_int(value, minimum, maximum, fallback):
    try:
        number = int(value)
    except (TypeError, ValueError):
        return fallback
    return max(minimum, min(maximum, number))


def normalize(raw):
    """把任意输入收敛成合法设置；非法字段用默认值兜底。"""
    settings = default_settings()
    if not isinstance(raw, dict):
        return settings

    try:
        settings["times"] = parse_times(raw.get("times"))
    except ValueError:
        settings["times"] = list(DEFAULT_TIMES)

    settings["retry_attempts"] = _clamp_int(
        raw.get("retry_attempts"), 1, MAX_RETRY_ATTEMPTS, DEFAULT_RETRY_ATTEMPTS
    )
    settings["retry_delay_minutes"] = _clamp_int(
        raw.get("retry_delay_minutes"), 0, MAX_RETRY_DELAY_MINUTES, DEFAULT_RETRY_DELAY_MINUTES
    )
    settings["autostart"] = bool(raw.get("autostart", False))
    settings["minimize_to_tray"] = bool(raw.get("minimize_to_tray", True))

    acc = raw.get("acc")
    settings["acc"] = str(acc).strip() if acc not in (None, "") else "30"
    return settings


def load_settings(path):
    """读取设置；文件不存在或损坏时返回默认值。"""
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return normalize(json.load(handle))
    except (OSError, ValueError):
        return normalize({})


def save_settings(settings, path):
    """归一化后写盘，返回实际写入的内容。"""
    normalized = normalize(settings)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(normalized, handle, ensure_ascii=False, indent=2)
    return normalized
