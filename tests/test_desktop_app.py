"""桌面窗口的冒烟测试：能建起来、能存配置、能加时间点。

真正弹窗的操作（扫码、打开浏览器）不在这里触发。
"""

import json

import pytest

pytest.importorskip("tkinter")

import tkinter as tk

import desktop.app as app_module


@pytest.fixture(scope="module")
def tk_root():
    """整个模块只建一个 Tk 根窗口，每个用例用 Toplevel，避免反复建根导致 Tcl 资源耗尽。"""
    try:
        root = tk.Tk()
    except tk.TclError:
        pytest.skip("当前环境没有图形界面")
    root.withdraw()
    yield root
    try:
        root.destroy()
    except tk.TclError:
        pass


@pytest.fixture
def app(tmp_path, monkeypatch, tk_root):
    (tmp_path / "logs").mkdir()

    monkeypatch.setattr(app_module.paths, "ensure_data_dir", lambda: tmp_path)
    monkeypatch.setattr(app_module.paths, "config_path", lambda: tmp_path / "data.json")
    monkeypatch.setattr(app_module.paths, "settings_path", lambda: tmp_path / "settings.json")
    monkeypatch.setattr(app_module.paths, "log_dir", lambda: tmp_path / "logs")
    monkeypatch.setattr(app_module.autostart, "is_autostart_enabled", lambda: False)
    monkeypatch.setattr(app_module.TrayController, "start", lambda self: False)
    monkeypatch.setattr(app_module.SigninApp, "_start_scheduler", lambda self, quiet=False: None)

    for name in ("showinfo", "showwarning", "showerror"):
        monkeypatch.setattr(app_module.messagebox, name, lambda *a, **k: None)
    monkeypatch.setattr(app_module.messagebox, "askyesno", lambda *a, **k: True)

    window = tk.Toplevel(tk_root)
    window.withdraw()
    instance = app_module.SigninApp(window)
    yield instance
    instance.destroy()
    try:
        window.destroy()
    except tk.TclError:
        pass


def test_parse_coord_accepts_common_separators():
    assert app_module.parse_coord("39.90,116.39") == ("39.90", "116.39")
    assert app_module.parse_coord("39.90 116.39") == ("39.90", "116.39")
    assert app_module.parse_coord("39.90，116.39") == ("39.90", "116.39")
    assert app_module.parse_coord("39.90|116.39") == ("39.90", "116.39")


def test_parse_coord_rejects_incomplete():
    assert app_module.parse_coord("39.90") == (None, None)
    assert app_module.parse_coord("") == (None, None)


def test_window_builds_with_default_times(app):
    assert app.times_list.size() == 1
    assert app.times_list.get(0) == "21:31:00"
    assert str(app.btn_stop["state"]) == "disabled"


def test_add_time_persists_to_settings(app, tmp_path):
    app.new_time_var.set("07:05")

    app._add_time()

    saved = json.loads((tmp_path / "settings.json").read_text(encoding="utf-8"))
    assert saved["times"] == ["07:05:00", "21:31:00"]
    assert app.times_list.size() == 2


def test_remove_last_time_is_rejected(app, tmp_path):
    app.times_list.selection_set(0)

    app._remove_time()

    assert app.times_list.size() == 1


def test_save_student_writes_config(app, tmp_path):
    app.students.append(
        {
            "name": "甲",
            "class": "123456",
            "lat": "",
            "lng": "",
            "acc": "30",
            "cookie": "remember_student_h=TOKEN",
            "QmsgKEY": "",
            "WXKey": "",
        }
    )
    app._refresh_tree()
    app.tree.selection_set("0")
    app.lat_var.set("39.90")
    app.lng_var.set("116.39")
    app.wx_var.set("SCTtest")

    app._save_student()

    saved = json.loads((tmp_path / "data.json").read_text(encoding="utf-8"))
    student = saved["students"][0]
    assert student["lat"] == "39.90"
    assert student["lng"] == "116.39"
    assert student["WXKey"] == "SCTtest"


def test_selecting_student_fills_form(app):
    app.students.append(
        {
            "name": "乙",
            "class": "654321",
            "lat": "30.1",
            "lng": "120.2",
            "acc": "25",
            "cookie": "remember_student_h=TOKEN",
            "QmsgKEY": "qq",
            "WXKey": "wx",
        }
    )
    app._refresh_tree()
    app.tree.selection_set("0")

    app._on_select()

    assert app.lat_var.get() == "30.1"
    assert app.acc_var.get() == "25"
    assert app.qq_var.get() == "qq"


def test_qr_success_adds_new_student(app, tmp_path):
    app.lat_var.set("39.90")
    app.lng_var.set("116.39")

    app._on_qr_success(
        "remember_student_h=NEWTOKEN",
        {"name": "丙"},
        {"class_id": "999888"},
    )

    saved = json.loads((tmp_path / "data.json").read_text(encoding="utf-8"))
    assert saved["students"][0]["name"] == "丙"
    assert saved["students"][0]["class"] == "999888"
    assert saved["students"][0]["cookie"] == "remember_student_h=NEWTOKEN"


def test_qr_success_updates_existing_student(app):
    app.students.append(
        {
            "name": "丙",
            "class": "old",
            "lat": "1",
            "lng": "2",
            "acc": "30",
            "cookie": "old-cookie",
            "QmsgKEY": "",
            "WXKey": "",
        }
    )

    app._on_qr_success("new-cookie", {"name": "丙"}, {"class_id": "999"})

    assert len(app.students) == 1
    assert app.students[0]["cookie"] == "new-cookie"
    assert app.students[0]["class"] == "999"


def test_delete_student_removes_and_saves(app, tmp_path):
    app.students.append(
        {
            "name": "丁",
            "class": "1",
            "lat": "1",
            "lng": "2",
            "acc": "30",
            "cookie": "c",
            "QmsgKEY": "",
            "WXKey": "",
        }
    )
    app._refresh_tree()
    app.tree.selection_set("0")

    app._delete_student()

    saved = json.loads((tmp_path / "data.json").read_text(encoding="utf-8"))
    assert saved["students"] == []
