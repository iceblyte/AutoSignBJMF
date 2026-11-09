"""settings.json 的读写与容错。"""

import json

import desktop.settings as settings


def test_defaults_when_file_missing(tmp_path):
    loaded = settings.load_settings(tmp_path / "nope.json")

    assert loaded["times"] == ["21:31:00"]
    assert loaded["retry_attempts"] == 3
    assert loaded["retry_delay_minutes"] == 5
    assert loaded["minimize_to_tray"] is True


def test_defaults_when_file_broken(tmp_path):
    path = tmp_path / "settings.json"
    path.write_text("{not json", encoding="utf-8")

    assert settings.load_settings(path)["times"] == ["21:31:00"]


def test_roundtrip_normalizes_times(tmp_path):
    path = tmp_path / "settings.json"

    saved = settings.save_settings({"times": ["21:31", "7:05"]}, path)

    assert saved["times"] == ["07:05:00", "21:31:00"]
    assert json.loads(path.read_text(encoding="utf-8"))["times"] == ["07:05:00", "21:31:00"]


def test_invalid_times_fall_back_to_default():
    assert settings.normalize({"times": ["25:00"]})["times"] == ["21:31:00"]
    assert settings.normalize({"times": []})["times"] == ["21:31:00"]
    assert settings.normalize({"times": "abc"})["times"] == ["21:31:00"]


def test_retry_values_are_clamped():
    assert settings.normalize({"retry_attempts": 0})["retry_attempts"] == 1
    assert settings.normalize({"retry_attempts": 999})["retry_attempts"] == 10
    assert settings.normalize({"retry_attempts": "x"})["retry_attempts"] == 3
    assert settings.normalize({"retry_delay_minutes": -5})["retry_delay_minutes"] == 0
    assert settings.normalize({"retry_delay_minutes": 999})["retry_delay_minutes"] == 120


def test_acc_blank_falls_back_to_30():
    assert settings.normalize({"acc": ""})["acc"] == "30"
    assert settings.normalize({"acc": " 50 "})["acc"] == "50"


def test_non_dict_input_returns_defaults():
    assert settings.normalize("oops")["times"] == ["21:31:00"]
    assert settings.normalize(None)["times"] == ["21:31:00"]


def test_bool_flags_are_coerced():
    normalized = settings.normalize({"autostart": 1, "minimize_to_tray": 0})

    assert normalized["autostart"] is True
    assert normalized["minimize_to_tray"] is False
