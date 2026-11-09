"""BJMF.py 只做入口适配：把退出码、中断和 3 秒收尾交给命令行。

具体签到流程的测试在 test_runner.py。
"""

import os
import runpy

import pytest

import utils.runner

from conftest import ROOT_DIR


def _run_bjmf(monkeypatch, behavior):
    """执行 BJMF.py，返回 (退出码, run_once 收到的配置路径)。"""
    import time

    recorded = {}
    monkeypatch.setattr(time, "sleep", lambda *_: None)

    def fake_run_once(path):
        recorded["path"] = path
        return behavior()

    monkeypatch.setattr(utils.runner, "run_once", fake_run_once)
    try:
        runpy.run_path(os.path.join(ROOT_DIR, "BJMF.py"), run_name="__main__")
        recorded["exit_code"] = None
    except SystemExit as exit_signal:
        recorded["exit_code"] = exit_signal.code
    return recorded


def test_bjmf_exits_with_runner_code(monkeypatch):
    recorded = _run_bjmf(monkeypatch, lambda: 0)

    assert recorded["exit_code"] == 0
    assert recorded["path"] == os.path.join(ROOT_DIR, "data.json")


def test_bjmf_propagates_failure_code(monkeypatch):
    recorded = _run_bjmf(monkeypatch, lambda: 1)

    assert recorded["exit_code"] == 1


def test_bjmf_maps_keyboard_interrupt_to_130(monkeypatch):
    def interrupted():
        raise KeyboardInterrupt

    recorded = _run_bjmf(monkeypatch, interrupted)

    assert recorded["exit_code"] == 130


def test_bjmf_uses_script_directory_config(monkeypatch):
    recorded = _run_bjmf(monkeypatch, lambda: 0)

    assert os.path.dirname(recorded["path"]) == ROOT_DIR


@pytest.mark.parametrize("code", [0, 1, 2])
def test_exit_code_passthrough(monkeypatch, code):
    assert _run_bjmf(monkeypatch, lambda: code)["exit_code"] == code
