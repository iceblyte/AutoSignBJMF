import json
import os
import runpy
import shutil
import sys
import types

import pytest

from conftest import ROOT_DIR


@pytest.fixture
def fake_utils(monkeypatch):
    """把 BJMF.py 依赖的 utils 包整体替换成可控桩件，避免任何真实网络请求。"""
    module = types.ModuleType("utils")
    recorded = {"summary_calls": [], "task_calls": [], "saved": [], "exit_code": None}

    state = {
        "user_info": ({"name": "测试用户"}, {"class_id": "123456"}),
        "task_status": "success",
        "task_status_by_name": {},
    }

    def _save_config(data, path):
        recorded["saved"].append(json.loads(json.dumps(data)))
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(data, handle, ensure_ascii=False)

    module.load_config = lambda path: json.loads(open(path, encoding="utf-8").read())
    module.save_config = _save_config
    module.get_students = lambda path=None: []
    module.get_user_and_class_info = lambda student: state["user_info"]
    module.sendQQmessage = lambda *a, **k: None
    module.wx_send = lambda *a, **k: None
    module.send_summary_notification = lambda key, results: recorded["summary_calls"].append((key, results))
    def _task(student):
        recorded["task_calls"].append(student)
        status = state["task_status_by_name"].get(student["name"], state["task_status"])
        return student["name"], status

    module.Task = _task

    monkeypatch.setitem(sys.modules, "utils", module)
    return recorded, state


@pytest.fixture
def run_bjmf(tmp_path, fake_utils):
    recorded, _state = fake_utils
    script = tmp_path / "BJMF.py"

    def _run(students):
        shutil.copyfile(os.path.join(ROOT_DIR, "BJMF.py"), script)
        (tmp_path / "data.json").write_text(
            json.dumps({"students": students}, ensure_ascii=False), encoding="utf-8"
        )
        try:
            runpy.run_path(str(script), run_name="__main__")
            recorded["exit_code"] = 0
        except SystemExit as exit_signal:
            recorded["exit_code"] = exit_signal.code if exit_signal.code is not None else 0
        return json.loads((tmp_path / "data.json").read_text(encoding="utf-8"))

    return _run


def make_student(cls=None, **overrides):
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
    if cls is not None:
        base["class"] = cls
    return base


def test_summary_uses_wxkey_from_student_object(fake_utils, run_bjmf):
    """回归：WXKey 存在每个学生配置里，顶层字段已废弃。"""
    recorded, _ = fake_utils
    run_bjmf([make_student(WXKey=""), make_student(WXKey="SCT_second"), make_student(WXKey="SCT_third")])

    assert recorded["summary_calls"][0][0] == "SCT_second"


def test_summary_skipped_without_any_wxkey(fake_utils, run_bjmf):
    recorded, _ = fake_utils
    run_bjmf([make_student()])

    assert recorded["summary_calls"] == []


def test_summary_receives_all_results(fake_utils, run_bjmf):
    recorded, _ = fake_utils
    run_bjmf([make_student(name="甲", WXKey="SCTx"), make_student(name="乙")])

    results = recorded["summary_calls"][0][1]
    assert results == [("甲", "success"), ("乙", "success")]


def test_empty_class_is_filled_and_saved(fake_utils, run_bjmf):
    recorded, state = fake_utils
    state["user_info"] = ({"name": "测试用户"}, {"class_id": "123456"})

    saved = run_bjmf([make_student(cls="")])

    assert saved["students"][0]["class"] == "123456"
    assert recorded["saved"][0]["students"][0]["class"] == "123456"


def test_skip_student_when_cookie_invalid(fake_utils, run_bjmf):
    recorded, state = fake_utils
    state["user_info"] = ({"name": "未找到"}, {})

    run_bjmf([make_student(cls="", WXKey="SCTx")])

    assert recorded["task_calls"] == []
    assert recorded["summary_calls"][0][1] == [("甲", "skip")]


def test_skip_student_when_class_unavailable(fake_utils, run_bjmf):
    recorded, state = fake_utils
    state["user_info"] = ({"name": "测试用户"}, {"class_id": "未找到"})

    run_bjmf([make_student(cls="", WXKey="SCTx")])

    assert recorded["task_calls"] == []
    assert recorded["summary_calls"][0][1] == [("甲", "skip")]


# --- 退出码：Actions 的重试逻辑依赖非零退出码，全部成功退出 0 会让重试永远不触发 ---

@pytest.mark.parametrize("status", ["skip", "error"])
def test_exit_nonzero_when_every_student_failed(fake_utils, run_bjmf, status):
    recorded, state = fake_utils
    state["task_status"] = status

    run_bjmf([make_student(name="甲"), make_student(name="乙")])

    assert recorded["exit_code"] == 1


def test_exit_zero_when_at_least_one_succeeded(fake_utils, run_bjmf):
    recorded, state = fake_utils
    state["task_status_by_name"] = {"甲": "skip", "乙": "success"}

    run_bjmf([make_student(name="甲"), make_student(name="乙")])

    assert recorded["exit_code"] == 0


def test_exit_zero_when_nothing_to_sign(fake_utils, run_bjmf):
    """当天确实没有签到项不算故障，不应该触发重试和失败告警。"""
    recorded, state = fake_utils
    state["task_status"] = "no_sign_in"

    run_bjmf([make_student(name="甲")])

    assert recorded["exit_code"] == 0


def test_exit_zero_when_no_students(fake_utils, run_bjmf):
    recorded, _state = fake_utils

    run_bjmf([])

    assert recorded["exit_code"] == 0
