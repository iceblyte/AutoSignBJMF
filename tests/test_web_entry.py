import importlib.util
import json
import os
import sys
import types

from conftest import WEB_SIGNIN_DIR


def _load_run_module(monkeypatch):
    """加载 web_signin/run.py，但用桩件替换 create_app，避免真的起调度和数据库。"""
    stub = types.ModuleType("app")
    stub.create_app = lambda: object()
    monkeypatch.setitem(sys.modules, "app", stub)

    spec = importlib.util.spec_from_file_location(
        "bjmf_web_run", os.path.join(WEB_SIGNIN_DIR, "run.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_default_host_is_loopback_only(monkeypatch):
    monkeypatch.delenv("WEB_SIGNIN_HOST", raising=False)
    monkeypatch.delenv("WEB_SIGNIN_PORT", raising=False)
    run = _load_run_module(monkeypatch)

    assert run._load_host_from_config() == "127.0.0.1"


def test_env_overrides_host(monkeypatch):
    monkeypatch.setenv("WEB_SIGNIN_HOST", "0.0.0.0")
    run = _load_run_module(monkeypatch)

    assert run._load_host_from_config() == "0.0.0.0"


def test_config_file_host(monkeypatch, tmp_path):
    monkeypatch.delenv("WEB_SIGNIN_HOST", raising=False)
    run = _load_run_module(monkeypatch)
    (tmp_path / "config.json").write_text(json.dumps({"host": "0.0.0.0", "port": 9988}), encoding="utf-8")

    monkeypatch.setattr(run, "_config_path", lambda: str(tmp_path / "config.json"))

    assert run._load_host_from_config() == "0.0.0.0"
    assert run._load_port_from_config() == 9988


def test_invalid_host_falls_back_to_loopback(monkeypatch):
    monkeypatch.setenv("WEB_SIGNIN_HOST", "not an ip")
    run = _load_run_module(monkeypatch)

    assert run._load_host_from_config() == "127.0.0.1"


def test_port_default_and_override(monkeypatch):
    monkeypatch.delenv("WEB_SIGNIN_PORT", raising=False)
    run = _load_run_module(monkeypatch)
    assert run._load_port_from_config() == 9988

    monkeypatch.setenv("WEB_SIGNIN_PORT", "8123")
    assert run._load_port_from_config() == 8123

    monkeypatch.setenv("WEB_SIGNIN_PORT", "70000")
    assert run._load_port_from_config() == 9988
