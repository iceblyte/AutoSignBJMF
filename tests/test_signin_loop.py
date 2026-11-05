import subprocess
from datetime import datetime, timedelta

import pytest

import signin_loop


def test_parse_times_normalizes_and_sorts():
    assert signin_loop.parse_times("21:31, 7:30:05") == ["07:30:05", "21:31:00"]


def test_parse_times_dedupes():
    assert signin_loop.parse_times("08:00,08:00:00") == ["08:00:00"]


@pytest.mark.parametrize("raw", ["", "   ", "25:00", "08:60", "abc", "8", "08:00:99"])
def test_parse_times_rejects_invalid(raw):
    with pytest.raises(ValueError):
        signin_loop.parse_times(raw)


def test_parse_times_accepts_single_time():
    assert signin_loop.parse_times("21:31") == ["21:31:00"]


def test_next_fire_returns_today_when_early():
    now = datetime(2026, 11, 5, 6, 0, 0)
    assert signin_loop.next_fire(["07:30:00", "21:31:00"], now) == datetime(2026, 11, 5, 7, 30, 0)


def test_next_fire_returns_later_slot_same_day():
    now = datetime(2026, 11, 5, 12, 0, 0)
    assert signin_loop.next_fire(["07:30:00", "21:31:00"], now) == datetime(2026, 11, 5, 21, 31, 0)


def test_next_fire_rolls_to_next_day():
    now = datetime(2026, 11, 5, 23, 59, 0)
    assert signin_loop.next_fire(["07:30:00", "21:31:00"], now) == datetime(2026, 11, 6, 7, 30, 0)


def test_next_fire_is_strictly_after_now():
    now = datetime(2026, 11, 5, 7, 30, 0)
    assert signin_loop.next_fire(["07:30:00"], now) == datetime(2026, 11, 6, 7, 30, 0)


def test_build_command_uses_current_interpreter():
    cmd = signin_loop.build_command("/usr/bin/python3", "/repo/BJMF.py")
    assert cmd == ["/usr/bin/python3", "/repo/BJMF.py"]


def test_wait_until_sleeps_in_short_chunks():
    target = datetime(2026, 11, 5, 21, 31, 0)
    state = {"now": target - timedelta(seconds=130)}
    slept = []

    signin_loop.wait_until(
        target,
        now_fn=lambda: state["now"],
        sleep_fn=lambda seconds: (slept.append(seconds), state.__setitem__("now", state["now"] + timedelta(seconds=seconds))),
    )

    assert max(slept) <= 60
    assert state["now"] >= target


def test_once_runs_sign_in_and_propagates_returncode(monkeypatch):
    calls = []

    def fake_run(cmd, **kwargs):
        calls.append(cmd)
        return subprocess.CompletedProcess(cmd, 1)

    monkeypatch.setattr(signin_loop.subprocess, "run", fake_run)

    code = signin_loop.main(["--once"])

    assert code == 1
    assert calls[0][1].endswith("BJMF.py")


def test_loop_requires_times(monkeypatch, capsys):
    assert signin_loop.main([]) == 2


def test_loop_runs_each_scheduled_time(monkeypatch):
    fired = []

    monkeypatch.setattr(signin_loop, "signin", lambda *a, **k: fired.append(a) or 0)
    monkeypatch.setattr(signin_loop.subprocess, "run", lambda cmd, **k: subprocess.CompletedProcess(cmd, 0))

    def fake_next_fire(_times, now):
        if len(fired) >= 2:
            raise KeyboardInterrupt
        return datetime(2026, 11, 5, 21, 31)

    monkeypatch.setattr(signin_loop, "next_fire", fake_next_fire)
    monkeypatch.setattr(signin_loop, "wait_until", lambda *a, **k: None)

    assert signin_loop.main(["--times", "21:31"]) == 0
    assert len(fired) == 2


def test_loop_survives_failed_sign_in(monkeypatch):
    """单次签到失败不能让常驻进程退出——否则第二天就不会再跑了。"""
    fired = []

    def fake_signin(*a, **k):
        fired.append(a)
        if len(fired) == 1:
            raise RuntimeError("网络异常")
        if len(fired) >= 2:
            raise KeyboardInterrupt
        return 0

    monkeypatch.setattr(signin_loop, "signin", fake_signin)
    monkeypatch.setattr(signin_loop, "next_fire", lambda _t, _n: datetime(2026, 11, 5, 21, 31))
    monkeypatch.setattr(signin_loop, "wait_until", lambda *a, **k: None)

    signin_loop.main(["--times", "21:31"])

    assert len(fired) == 2
