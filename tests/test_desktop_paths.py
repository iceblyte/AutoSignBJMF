"""数据目录解析：绿色版优先、否则用户目录，以及老配置迁移。"""

import sys
from pathlib import Path

import desktop.paths as paths


def test_is_frozen_reads_sys_flag(monkeypatch):
    monkeypatch.delattr(sys, "frozen", raising=False)
    assert paths.is_frozen() is False

    monkeypatch.setattr(sys, "frozen", True, raising=False)
    assert paths.is_frozen() is True


def test_portable_dir_wins_when_exe_has_data(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)
    (tmp_path / "data").mkdir()

    assert paths.data_dir() == tmp_path / "data"


def test_falls_back_to_home_when_no_portable_dir(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)
    monkeypatch.setattr(paths, "home_data_dir", lambda: tmp_path / "home")

    assert paths.data_dir() == tmp_path / "home"


def test_home_data_dir_is_under_user_folder():
    assert paths.home_data_dir().name == "AutoSignBJMF"
    assert str(paths.home_data_dir().parent) == str(Path.home())


def test_ensure_data_dir_creates_logs(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)
    monkeypatch.setattr(paths, "home_data_dir", lambda: tmp_path / "home")

    target = paths.ensure_data_dir()

    assert target.is_dir()
    assert (target / "logs").is_dir()


def test_ensure_data_dir_migrates_legacy_config(tmp_path, monkeypatch):
    exe = tmp_path / "exe"
    exe.mkdir()
    (exe / "data.json").write_text('{"students": []}', encoding="utf-8")
    home = tmp_path / "home"
    monkeypatch.setattr(paths, "exe_dir", lambda: exe)
    monkeypatch.setattr(paths, "home_data_dir", lambda: home)

    target = paths.ensure_data_dir()

    assert (target / "data.json").read_text(encoding="utf-8") == '{"students": []}'


def test_migration_does_not_overwrite_existing_config(tmp_path, monkeypatch):
    exe = tmp_path / "exe"
    exe.mkdir()
    (exe / "data.json").write_text("legacy", encoding="utf-8")
    home = tmp_path / "home"
    home.mkdir()
    (home / "data.json").write_text("current", encoding="utf-8")
    monkeypatch.setattr(paths, "exe_dir", lambda: exe)
    monkeypatch.setattr(paths, "home_data_dir", lambda: home)

    target = paths.ensure_data_dir()

    assert (target / "data.json").read_text(encoding="utf-8") == "current"


def test_portable_mode_uses_its_own_config(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)
    (tmp_path / "data").mkdir()

    assert paths.config_path() == tmp_path / "data" / "data.json"
    assert paths.settings_path() == tmp_path / "data" / "settings.json"
    assert paths.log_dir() == tmp_path / "data" / "logs"


def test_is_portable_true_when_data_dir_exists(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)
    (tmp_path / "data").mkdir()

    assert paths.is_portable() is True


def test_is_portable_false_for_bare_exe(tmp_path, monkeypatch):
    """单独拷出来的 exe 没有 data/，应判定为用户目录模式。"""
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)

    assert paths.is_portable() is False


def test_is_portable_true_when_data_dir_exists(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)
    (tmp_path / "data").mkdir()

    assert paths.is_portable() is True


def test_is_portable_false_for_bare_exe(tmp_path, monkeypatch):
    """单独拷出来的 exe 没有 data/，应判定为用户目录模式。"""
    monkeypatch.setattr(paths, "exe_dir", lambda: tmp_path)

    assert paths.is_portable() is False
