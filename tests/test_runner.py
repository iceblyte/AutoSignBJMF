"""utils/runner.py 的单次签到流程测试（全程桩件，不发真实请求）。"""

import json

import pytest

import utils.runner as runner

from conftest import ROOT_DIR


def _student(**overrides):
    base = {
        "name": "甲",
        "class": "123456",
        "lat": "30.1",
        "lng": "120.1",
        "acc": "30",
        "cookie": "remember_student_hash=TOKEN",
        "QmsgKEY": "",
        "WXKey": "",
    }
    base.update(overrides)
    return base


def _write_config(tmp_path, students):
    path = tmp_path / "data.json"
    path.write_text(json.dumps({"students": students}, ensure_ascii=False), encoding="utf-8")
    return str(path)


@pytest.fixture
def env(monkeypatch):
    """把 runner 依赖的函数全部换成可观测的桩件。"""
    state = {
        "user_info": ({"name": "测试用户"}, {"class_id": "123456"}),
        "task_status": "success",
        "task_status_by_name": {},
    }
    calls = {"summary": [], "tasks": [], "saved": []}

    def load_config(path):
        with open(path, encoding="utf-8") as handle:
            return json.load(handle)

    def save_config(data, path):
        calls["saved"].append(json.loads(json.dumps(data)))
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(data, handle, ensure_ascii=False)

    def fake_task(student):
        calls["tasks"].append(student)
        status = state["task_status_by_name"].get(student["name"], state["task_status"])
        return student["name"], status

    monkeypatch.setattr(runner, "load_config", load_config)
    monkeypatch.setattr(runner, "save_config", save_config)
    monkeypatch.setattr(runner, "get_user_and_class_info", lambda student: state["user_info"])
    monkeypatch.setattr(
        runner, "send_summary_notification", lambda key, results: calls["summary"].append((key, results))
    )
    monkeypatch.setattr(runner, "Task", fake_task)
    return state, calls


def test_default_config_path_points_to_project_root():
    assert runner.default_config_path().endswith("data.json")
    assert runner.default_config_path().startswith(ROOT_DIR)


def test_summary_uses_first_non_empty_wxkey(env, tmp_path):
    state, calls = env
    path = _write_config(
        tmp_path,
        [_student(WXKey=""), _student(name="乙", WXKey="SCT_second"), _student(name="丙", WXKey="SCT_third")],
    )

    runner.run_once(path)

    assert calls["summary"][0][0] == "SCT_second"


def test_summary_skipped_without_wxkey(env, tmp_path):
    state, calls = env
    path = _write_config(tmp_path, [_student()])

    runner.run_once(path)

    assert calls["summary"] == []


def test_summary_receives_all_results(env, tmp_path):
    state, calls = env
    path = _write_config(tmp_path, [_student(WXKey="SCTx"), _student(name="乙")])

    runner.run_once(path)

    assert calls["summary"][0][1] == [("甲", "success"), ("乙", "success")]


def test_empty_class_is_filled_and_saved(env, tmp_path):
    state, calls = env
    state["user_info"] = ({"name": "测试用户"}, {"class_id": "123456"})
    path = _write_config(tmp_path, [_student(**{"class": ""})])

    saved = runner.run_once(path)

    assert json.loads(open(path, encoding="utf-8").read())["students"][0]["class"] == "123456"
    assert calls["saved"][0]["students"][0]["class"] == "123456"
    assert saved == 0


def test_skip_when_cookie_invalid(env, tmp_path):
    state, calls = env
    state["user_info"] = ({"name": "未找到"}, {})
    path = _write_config(tmp_path, [_student(WXKey="SCTx", **{"class": ""})])

    code = runner.run_once(path)

    assert calls["tasks"] == []
    assert calls["summary"][0][1] == [("甲", "skip")]
    assert code == 1


def test_skip_when_class_unavailable(env, tmp_path):
    state, calls = env
    state["user_info"] = ({"name": "测试用户"}, {"class_id": "未找到"})
    path = _write_config(tmp_path, [_student(WXKey="SCTx", **{"class": ""})])

    code = runner.run_once(path)

    assert calls["tasks"] == []
    assert calls["summary"][0][1] == [("甲", "skip")]
    assert code == 1


@pytest.mark.parametrize("status", ["skip", "error"])
def test_exit_nonzero_when_every_student_failed(env, tmp_path, status):
    state, calls = env
    state["task_status"] = status
    path = _write_config(tmp_path, [_student(), _student(name="乙")])

    assert runner.run_once(path) == 1


def test_exit_zero_when_at_least_one_succeeded(env, tmp_path):
    state, calls = env
    state["task_status_by_name"] = {"甲": "skip", "乙": "success"}
    path = _write_config(tmp_path, [_student(), _student(name="乙")])

    assert runner.run_once(path) == 0


def test_exit_zero_when_nothing_to_sign(env, tmp_path):
    state, calls = env
    state["task_status"] = "no_sign_in"
    path = _write_config(tmp_path, [_student()])

    assert runner.run_once(path) == 0


def test_exit_zero_when_no_students(env, tmp_path):
    state, calls = env
    path = _write_config(tmp_path, [])

    assert runner.run_once(path) == 0


def test_unexpected_error_returns_one(env, tmp_path, monkeypatch):
    state, calls = env

    def boom(student):
        raise RuntimeError("意外错误")

    monkeypatch.setattr(runner, "Task", boom)
    path = _write_config(tmp_path, [_student()])

    assert runner.run_once(path) == 1


def test_keyboard_interrupt_propagates(env, tmp_path, monkeypatch):
    state, calls = env

    def interrupt(student):
        raise KeyboardInterrupt

    monkeypatch.setattr(runner, "Task", interrupt)
    path = _write_config(tmp_path, [_student()])

    with pytest.raises(KeyboardInterrupt):
        runner.run_once(path)
