"""DailyScheduler 的调度、重试与暂停语义。

用假时钟（wait 直接推进时间）避免真的等，线程只用于验证启停与暂停。
"""

import threading
import time
from datetime import datetime, timedelta

import pytest

from desktop.scheduler import DailyScheduler


class FakeClock:
    def __init__(self, start):
        self.current = start
        self.waits = []

    def now(self):
        return self.current

    def wait(self, seconds):
        self.waits.append(seconds)
        self.current += timedelta(seconds=seconds)
        return True


def _scheduler(codes, times=("21:31:00",), start=datetime(2026, 1, 1, 20, 0, 0), **kwargs):
    clock = FakeClock(start)
    calls = []

    def run_once():
        index = len(calls)
        calls.append(clock.now())
        return codes[index] if index < len(codes) else 0

    scheduler = DailyScheduler(
        run_once, lambda: list(times), now_fn=clock.now, wait_fn=clock.wait, **kwargs
    )
    return scheduler, calls, clock


def _wait_until(predicate, timeout=3):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.01)
    return False


def test_runs_at_scheduled_local_time():
    scheduler, calls, clock = _scheduler([0], retry_attempts=1)

    scheduler.start()
    assert _wait_until(lambda: len(calls) >= 1)
    scheduler.stop()

    assert calls[0].strftime("%H:%M:%S") == "21:31:00"


def test_success_does_not_retry():
    scheduler, calls, clock = _scheduler([0], retry_attempts=3, retry_delay_seconds=60)

    assert scheduler._run_with_retry() == 0

    assert len(calls) == 1
    assert clock.waits == []


def test_retries_then_succeeds():
    scheduler, calls, clock = _scheduler([1, 0], retry_attempts=3, retry_delay_seconds=60)

    assert scheduler._run_with_retry() == 0

    assert len(calls) == 2
    assert clock.waits == [60]


def test_gives_up_after_max_attempts():
    scheduler, calls, clock = _scheduler([1, 1, 1, 0], retry_attempts=3, retry_delay_seconds=60)

    assert scheduler._run_with_retry() == 1

    assert len(calls) == 3
    assert clock.waits == [60, 60]
    assert scheduler.last_exit_code == 1


def test_events_are_emitted_in_order():
    events = []
    scheduler, calls, clock = _scheduler(
        [1, 0],
        retry_attempts=2,
        retry_delay_seconds=30,
        on_event=lambda name, payload: events.append((name, payload.get("attempt"))),
    )

    scheduler._run_with_retry()

    assert events == [
        ("run_start", 1),
        ("run_done", 1),
        ("run_start", 2),
        ("run_done", 2),
    ]


def test_exception_inside_run_once_counts_as_failure():
    def boom():
        raise RuntimeError("网络断了")

    events = []
    scheduler = DailyScheduler(
        boom, lambda: ["21:31:00"], retry_attempts=1, on_event=lambda n, p: events.append(n)
    )

    assert scheduler._run_with_retry() == 1
    assert scheduler.last_exit_code == 1
    assert "网络断了" in scheduler.last_error
    assert "error" in events


def test_run_now_returns_code_and_records_state():
    scheduler, calls, clock = _scheduler([0])

    assert scheduler.run_now() == 0
    assert scheduler.last_exit_code == 0
    assert len(calls) == 1


def test_pause_skips_firing_time():
    clock = FakeClock(datetime(2026, 1, 1, 20, 0, 0))
    entered = threading.Event()
    release = threading.Event()
    calls = []
    events = []

    def run_once():
        calls.append(clock.now())
        entered.set()
        release.wait(2)
        return 0

    scheduler = DailyScheduler(
        run_once,
        lambda: ["21:31:00"],
        now_fn=clock.now,
        wait_fn=clock.wait,
        retry_attempts=1,
        on_event=lambda name, payload: events.append(name),
    )

    scheduler.start()
    assert entered.wait(2)

    scheduler.pause()
    release.set()

    assert _wait_until(lambda: "skipped" in events)
    scheduler.stop()

    assert len(calls) == 1
    assert scheduler.paused is True


def test_resume_clears_pause_flag():
    scheduler, calls, clock = _scheduler([0])

    scheduler.pause()
    assert scheduler.paused is True

    scheduler.resume()
    assert scheduler.paused is False


def test_stop_interrupts_long_wait():
    scheduler = DailyScheduler(
        lambda: 0,
        lambda: ["23:59:59"],
        retry_attempts=1,
        chunk_seconds=60,
    )
    scheduler.start()
    assert _wait_until(lambda: scheduler.running)
    time.sleep(0.05)

    started = time.time()
    scheduler.stop()

    assert time.time() - started < 2
    assert scheduler.running is False
    assert scheduler.next_run_at is None


def test_stop_before_start_is_safe():
    scheduler, calls, clock = _scheduler([0])

    scheduler.stop()

    assert scheduler.running is False
    assert calls == []


def test_rolls_over_to_next_day_when_time_passed():
    scheduler, calls, clock = _scheduler([0], start=datetime(2026, 1, 1, 23, 0, 0))

    scheduler.start()
    assert _wait_until(lambda: len(calls) >= 1)
    scheduler.stop()

    # 启动时 23:00 已过 21:31，第一次必须落在第二天
    assert calls[0] == datetime(2026, 1, 2, 21, 31, 0)


def test_times_provider_error_does_not_kill_loop(monkeypatch):
    attempts = {"count": 0}

    def broken_times():
        attempts["count"] += 1
        if attempts["count"] > 1:
            raise RuntimeError("配置坏了")
        return ["21:31:00"]

    events = []
    clock = FakeClock(datetime(2026, 1, 1, 20, 0, 0))
    scheduler = DailyScheduler(
        lambda: 0,
        broken_times,
        now_fn=clock.now,
        wait_fn=clock.wait,
        retry_attempts=1,
        on_event=lambda name, payload: events.append(name),
    )

    scheduler.start()
    assert _wait_until(lambda: "error" in events)
    scheduler.stop()

    assert scheduler.running is False
