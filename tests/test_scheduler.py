import json
import os
import subprocess
import sys
from datetime import datetime, timezone

import pytest

from conftest import ROOT_DIR, WEB_SIGNIN_DIR

import app.db as web_db
import app.scheduler as web_scheduler
from app.models import Task, TaskLog


def test_web_entry_can_import_root_utils():
    """以 web_signin 为工作目录启动时（run.py / Docker CMD 的真实形态），
    调度任务必须仍然能导入项目根的 utils 包。"""
    code = "import app.scheduler; import utils.attendance; import utils.notification; print('OK')"
    proc = subprocess.run(
        [sys.executable, "-c", code],
        cwd=WEB_SIGNIN_DIR,
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 0, proc.stderr
    assert proc.stdout.strip().endswith("OK")


@pytest.mark.parametrize(
    "raw,expected",
    [
        ('["07:30", "21:31:05"]', ["07:30:00", "21:31:05"]),
        ('["25:00"]', []),
        ('["07:30:70"]', []),
        ("not-json", []),
        ('{"a": 1}', []),
        ("[]", []),
    ],
)
def test_parse_times(raw, expected):
    assert web_scheduler._parse_times(raw) == expected


class FakeApp:
    def __init__(self, scheduler):
        self.config = {"SCHEDULER": scheduler}


@pytest.fixture
def scheduler_instance():
    from apscheduler.schedulers.background import BackgroundScheduler

    sched = BackgroundScheduler(timezone="Asia/Shanghai")
    yield sched
    if sched.running:
        sched.shutdown(wait=False)


@pytest.fixture
def temp_db(tmp_path, monkeypatch):
    monkeypatch.setattr(web_db, "DB_PATH", tmp_path / "test.db")
    web_db.init_db()
    return tmp_path / "test.db"


def _make_task(**kwargs):
    defaults = dict(
        name="测试用户",
        class_id="123456",
        cookie="remember_student_hash=TOKEN",
        lat="30.1",
        lng="120.1",
        acc="30",
        wx_key="",
        qq_key="",
        times=json.dumps(["21:31:00"]),
        enabled=True,
    )
    defaults.update(kwargs)
    return Task(**defaults)


def test_register_task_jobs_uses_beijing_time(scheduler_instance):
    task = _make_task(id=1)
    web_scheduler.register_task_jobs(FakeApp(scheduler_instance), task)

    jobs = scheduler_instance.get_jobs()
    assert [j.id for j in jobs] == ["task_1_213100"]

    nxt = jobs[0].trigger.get_next_fire_time(None, datetime(2026, 1, 1, tzinfo=timezone.utc))
    assert nxt.astimezone(timezone.utc).strftime("%H:%M") == "13:31"


def test_register_task_jobs_replaces_existing(scheduler_instance):
    task = _make_task(id=2, times=json.dumps(["08:00", "08:00"]))
    app_fake = FakeApp(scheduler_instance)
    web_scheduler.register_task_jobs(app_fake, task)
    web_scheduler.register_task_jobs(app_fake, task)

    assert len(scheduler_instance.get_jobs()) == 1


def test_unregister_only_touches_own_task(scheduler_instance):
    app_fake = FakeApp(scheduler_instance)
    web_scheduler.register_task_jobs(app_fake, _make_task(id=3))
    web_scheduler.register_task_jobs(app_fake, _make_task(id=4))

    web_scheduler.unregister_task_jobs(app_fake, 3)

    assert [j.id for j in scheduler_instance.get_jobs()] == ["task_4_213100"]


def test_run_task_job_logs_success(monkeypatch, temp_db):
    from sqlalchemy.orm import sessionmaker

    SessionLocal = sessionmaker(bind=web_db.get_engine())
    with SessionLocal() as db:
        task = _make_task(times=json.dumps(["21:31:00"]))
        db.add(task)
        db.commit()
        task_id = task.id

    calls = []

    def fake_attendance(student):
        calls.append(student)
        return student["name"], "success"

    sent = []
    monkeypatch.setattr(web_scheduler, "AttendanceTask", fake_attendance, raising=False)
    monkeypatch.setattr("utils.attendance.Task", fake_attendance)
    monkeypatch.setattr("utils.notification.wx_send", lambda key, msg: sent.append(("wx", key, msg)))
    monkeypatch.setattr("utils.notification.sendQQmessage", lambda key, msg: sent.append(("qq", key, msg)))

    web_scheduler.run_task_job(task_id)

    student = calls[0]
    assert student["cookie"] == "remember_student_hash=TOKEN"
    assert student["class"] == "123456"
    assert student["QmsgKEY"] == ""

    with SessionLocal() as db:
        log = db.query(TaskLog).one()
        assert log.status == "success"
        assert "班级标识: 123456" in log.message
        assert "签到成功（签到成功）" in log.message


@pytest.mark.parametrize(
    "status,expected_log_status",
    [
        ("success", "success"),
        ("already_signed", "success"),
        ("not_started", "skipped"),
        ("no_sign_in", "skipped"),
        ("skip", "skipped"),
        ("error", "error"),
    ],
)
def test_run_task_job_status_mapping(monkeypatch, temp_db, status, expected_log_status):
    from sqlalchemy.orm import sessionmaker

    SessionLocal = sessionmaker(bind=web_db.get_engine())
    with SessionLocal() as db:
        task = _make_task(times=json.dumps(["07:30:00"]), wx_key="SCTx")
        db.add(task)
        db.commit()
        task_id = task.id

    monkeypatch.setattr("utils.attendance.Task", lambda student: (student["name"], status))
    notified = []
    monkeypatch.setattr("utils.notification.wx_send", lambda key, msg: notified.append(msg))

    web_scheduler.run_task_job(task_id)

    with SessionLocal() as db:
        log = db.query(TaskLog).one()
        assert log.status == expected_log_status

    if status in ("success", "already_signed"):
        assert notified == ["签到成功！"]
    else:
        assert notified[0].startswith("签到失败，原因：")


def test_run_task_job_missing_task_is_noop(temp_db):
    web_scheduler.run_task_job(9999)


def test_cleanup_old_logs_removes_only_stale(temp_db):
    from datetime import timedelta

    from sqlalchemy.orm import sessionmaker

    SessionLocal = sessionmaker(bind=web_db.get_engine())
    with SessionLocal() as db:
        task = _make_task(times=json.dumps(["07:30:00"]))
        db.add(task)
        db.commit()
        db.add(TaskLog(task_id=task.id, status="success", message="旧", run_at=datetime.utcnow() - timedelta(days=8)))
        db.add(TaskLog(task_id=task.id, status="success", message="新", run_at=datetime.utcnow() - timedelta(days=1)))
        db.commit()

    with SessionLocal() as db:
        web_scheduler.cleanup_old_logs(db)
        assert [log.message for log in db.query(TaskLog).all()] == ["新"]


# --- 日志时区：调度按 Asia/Shanghai，日志正文也必须按北京时间展示，否则差 8 小时 ---


def test_to_beijing_shifts_utc_by_eight_hours():
    assert web_scheduler._to_beijing(datetime(2026, 1, 1, 13, 31, 5)).strftime("%H:%M:%S") == "21:31:05"


def test_to_beijing_handles_new_year_rollover():
    assert web_scheduler._to_beijing(datetime(2026, 12, 31, 17, 0, 0)).strftime("%Y-%m-%d %H:%M") == "2027-01-01 01:00"


def test_log_message_uses_beijing_time(monkeypatch, temp_db):
    """数据库里 run_at 存 UTC，但日志正文里给人看的时间必须是北京时间。"""
    from sqlalchemy.orm import sessionmaker

    class FrozenDatetime(datetime):
        @classmethod
        def utcnow(cls):
            return cls(2026, 1, 1, 13, 31, 5)

    monkeypatch.setattr(web_scheduler, "datetime", FrozenDatetime)

    SessionLocal = sessionmaker(bind=web_db.get_engine())
    with SessionLocal() as db:
        task = _make_task(times=json.dumps(["21:31:00"]))
        db.add(task)
        db.commit()
        task_id = task.id

    monkeypatch.setattr("utils.attendance.Task", lambda student: (student["name"], "success"))

    web_scheduler.run_task_job(task_id)

    with SessionLocal() as db:
        log = db.query(TaskLog).one()
        assert "21:31:05" in log.message
        assert "13:31:05" not in log.message

