import json

import utils.config_manager as config_manager


def test_load_missing_file_returns_empty_dict(tmp_path):
    assert config_manager.load_config(tmp_path / "nope.json") == {}


def test_load_broken_file_returns_empty_dict(tmp_path):
    path = tmp_path / "bad.json"
    path.write_text("{ not json", encoding="utf-8")
    assert config_manager.load_config(path) == {}


def test_save_then_load_roundtrip_keeps_chinese(tmp_path):
    path = tmp_path / "data.json"
    payload = {"students": [{"name": "张三", "class": "123456", "cookie": "a=1"}]}

    config_manager.save_config(payload, path)
    loaded = config_manager.load_config(path)

    assert loaded == payload
    raw = path.read_text(encoding="utf-8")
    assert "张三" in raw
    assert "\\u" not in raw


def test_get_students_returns_list(tmp_path):
    path = tmp_path / "data.json"
    config_manager.save_config({"students": [{"name": "甲"}, {"name": "乙"}]}, path)

    assert len(config_manager.get_students(path)) == 2


def test_get_students_empty_when_key_absent(tmp_path):
    path = tmp_path / "data.json"
    config_manager.save_config({}, path)

    assert config_manager.get_students(path) == []


def test_saved_file_is_valid_json(tmp_path):
    path = tmp_path / "data.json"
    config_manager.save_config({"students": []}, path)

    assert json.loads(path.read_text(encoding="utf-8")) == {"students": []}
