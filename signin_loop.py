"""常驻签到脚本：按本地时间每天定时执行 BJMF.py，不依赖终端保持打开。

用法：
    python signin_loop.py --times 07:30,21:31   # 常驻，每天到点执行
    python signin_loop.py --once                # 立即执行一次后退出，用来验证配置
"""

import argparse
import os
import subprocess
import sys
import time
from datetime import datetime, timedelta

SCRIPT_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "BJMF.py")

# 分片睡眠，保证 Ctrl+C 随时能停下来，也避免长睡眠被系统休眠打断后不再醒来
MAX_SLEEP_SECONDS = 60


def parse_times(raw):
    """把 "21:31, 7:30:05" 解析成排序去重后的 ["07:30:05", "21:31:00"]。"""
    times = []
    for chunk in (raw or "").split(","):
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


def wait_until(target, now_fn=datetime.now, sleep_fn=time.sleep):
    while True:
        remaining = (target - now_fn()).total_seconds()
        if remaining <= 0:
            return
        sleep_fn(min(remaining, MAX_SLEEP_SECONDS))


def build_command(python_executable, script_path):
    return [python_executable, script_path]


def signin(script_path=SCRIPT_PATH):
    return subprocess.run(build_command(sys.executable, script_path)).returncode


def main(argv=None):
    parser = argparse.ArgumentParser(description="按本地时间每天定时执行签到")
    parser.add_argument("--times", help="每天执行的时间点，逗号分隔，形如 07:30,21:31:05")
    parser.add_argument("--once", action="store_true", help="立即执行一次后退出，用于验证配置")
    args = parser.parse_args(argv)

    if args.once:
        return signin()

    try:
        times = parse_times(args.times)
    except ValueError as error:
        print(f"参数错误: {error}")
        return 2

    print(f"常驻签到已启动，每天执行时间点（本地时间）: {', '.join(times)}")
    try:
        while True:
            target = next_fire(times, datetime.now())
            print(f"下次执行: {target:%Y-%m-%d %H:%M:%S}")
            wait_until(target)
            try:
                print(f"本次签到退出码: {signin()}")
            except Exception as error:
                # 单次失败不能让进程退出，否则第二天就不会再跑了
                print(f"本次签到执行异常，等待下一个时间点: {error}")
    except KeyboardInterrupt:
        print("已停止常驻签到")
    return 0


if __name__ == "__main__":
    sys.exit(main())
