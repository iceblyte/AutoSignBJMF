"""时间点解析与下一次触发时间计算。

命令行常驻（signin_loop）与桌面端调度共用同一套逻辑，避免两处实现漂移。
"""

from datetime import datetime, timedelta


def parse_times(raw):
    """把 "21:31, 7:30:05" 解析成排序去重后的 ["07:30:05", "21:31:00"]。

    Args:
        raw: 逗号分隔的时间字符串，也接受已经是列表/元组的输入

    Returns:
        list[str]: 统一为 HH:MM:SS 的字符串列表

    Raises:
        ValueError: 格式错误、数值越界或没有任何有效时间点
    """
    if raw is None:
        raw = ""
    if isinstance(raw, (list, tuple)):
        chunks = [str(item) for item in raw]
    else:
        chunks = str(raw).split(",")

    times = []
    for chunk in chunks:
        chunk = chunk.strip()
        if not chunk:
            continue
        parts = chunk.split(":")
        if len(parts) not in (2, 3):
            raise ValueError(f"时间格式错误: {chunk}，应形如 HH:MM 或 HH:MM:SS")
        try:
            hour, minute = int(parts[0]), int(parts[1])
            second = int(parts[2]) if len(parts) == 3 else 0
        except ValueError:
            raise ValueError(f"时间格式错误: {chunk}，应形如 HH:MM 或 HH:MM:SS")
        if not (0 <= hour < 24 and 0 <= minute < 60 and 0 <= second < 60):
            raise ValueError(f"时间超出范围: {chunk}")
        times.append(f"{hour:02d}:{minute:02d}:{second:02d}")

    if not times:
        raise ValueError("至少需要一个时间点，例如 --times 21:31")
    return sorted(set(times))


def next_fire(times, now):
    """返回严格晚于 now 的下一个执行时间点。"""
    candidates = []
    for time_str in times:
        hour, minute, second = (int(part) for part in time_str.split(":"))
        for day_offset in (0, 1):
            day = (now + timedelta(days=day_offset)).date()
            candidate = datetime(day.year, day.month, day.day, hour, minute, second)
            if candidate > now:
                candidates.append(candidate)
                break
    return min(candidates)


def humanize_delta(seconds):
    """把剩余秒数转成「5 小时 39 分」这种给界面看的中文描述。"""
    seconds = int(max(0, seconds))
    if seconds < 60:
        return f"{seconds} 秒"
    minutes, seconds = divmod(seconds, 60)
    if minutes < 60:
        return f"{minutes} 分 {seconds} 秒" if seconds else f"{minutes} 分"
    hours, minutes = divmod(minutes, 60)
    if hours < 24:
        return f"{hours} 小时 {minutes} 分" if minutes else f"{hours} 小时"
    days, hours = divmod(hours, 24)
    return f"{days} 天 {hours} 小时" if hours else f"{days} 天"
