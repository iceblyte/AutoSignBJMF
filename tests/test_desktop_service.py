"""WebView 服务层测试：不建窗口，只验证状态与动作。"""

import json

import pytest

import desktop.paths as paths
import desktop.service as service_module


@pytest.fixture
def svc(tmp_path, monkeypatch):
    data_dir = tmp_path / "data"
    data_dir.mkdir()
    (data_dir / "data.json").write_text(
        json.dumps(
            {
                "students": [
                    {"name": "甲", "class": "1000001", "lat": "1.0", "lng": "2.0",
                     "acc": "30", "cookie": "remember_student_x=TOKEN", "WXKey": "", "QQKey": ""},
                ]
            },
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )
    (data_dir / "settings.json").write_text(
        json.dumps({"times": ["07:30:00"], "retry_attempts": 2, "retry_delay_minutes": 5}),
        encoding="utf-8",
    )

    monkeypatch.setattr(paths, "ensure_data_dir", lambda: data_dir)
    monkeypatch.setattr(paths, "config_path", lambda: data_dir / "data.json")
    monkeypatch.setattr(paths, "settings_path", lambda: data_dir / "settings.json")
    monkeypatch.setattr(paths, "is_portable", lambda: True)

    service = service_module.DesktopService()
    yield service
    import sys

    sys.stdout = sys.__stdout__
    sys.stderr = sys.__stderr__


def test_state_reports_student_and_settings(svc):
    state = svc.state()

    assert len(state["students"]) == 1
    assert state["student"]["name"] == "甲"
    assert state["settings"]["times"] == ["07:30:00"]
    assert state["data"]["portable"] is True


def test_add_and_remove_time_persists(svc, tmp_path):
    svc.add_time("21:31:00")
    assert "21:31:00" in svc.state()["settings"]["times"]

    svc.remove_time("21:31:00")
    assert svc.state()["settings"]["times"] == ["07:30:00"]

    saved = json.loads((tmp_path / "data" / "settings.json").read_text(encoding="utf-8"))
    assert saved["times"] == ["07:30:00"]


def test_add_time_rejects_bad_format(svc):
    result = svc.add_time("25:99")

    assert result["ok"] is False


def test_save_student_requires_coordinates(svc):
    result = svc.save_student({"lat": "", "lng": ""})

    assert result["ok"] is False


def test_save_student_writes_fields(svc, tmp_path):
    result = svc.save_student({"lat": "1.5", "lng": "2.5", "acc": "30", "wx": "K1", "qq": "K2"})

    assert result["ok"] is True
    student = json.loads((tmp_path / "data" / "data.json").read_text(encoding="utf-8"))["students"][0]
    assert student["lat"] == "1.5"
    assert student["WXKey"] == "K1"
    assert student["cookie"] == "remember_student_x=TOKEN"


def test_tick_contains_logs_and_status(svc):
    tick = svc.tick()

    assert "logs" in tick and isinstance(tick["logs"], list)
    assert "text" in tick
