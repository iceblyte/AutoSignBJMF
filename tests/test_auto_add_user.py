"""auto_add_user.py 的配置路径与日志脱敏测试。

这个脚本会把 data.json 内容打到 stdout，而 data.json 里含登录 Cookie 与推送 Key，
所以「写到哪个文件」和「打印什么」都必须固定住，不能跟着 CWD 或随手 print 跑偏。
"""

import os

import auto_add_user

from conftest import ROOT_DIR


def test_data_path_points_to_repo_root(monkeypatch, tmp_path):
    """无论从哪个目录启动，都读写脚本同级的 data.json（与 BJMF.py 保持一致）。"""
    monkeypatch.chdir(tmp_path)

    path = auto_add_user.data_path()

    assert os.path.isdir(os.path.dirname(path))
    assert os.path.dirname(path) == ROOT_DIR
    assert os.path.basename(path) == "data.json"


def test_mask_cookie_keeps_name_only():
    masked = auto_add_user.mask_cookie("remember_student_abc=SECRETVALUE; s=xyz")

    assert "SECRETVALUE" not in masked
    assert "xyz" not in masked
    assert masked == "remember_student_abc=****"


def test_mask_empty_cookie_is_empty():
    assert auto_add_user.mask_cookie("") == ""


def test_dump_config_hides_cookie_and_notification_keys():
    config = {
        "students": [
            {
                "name": "甲",
                "class": "123456",
                "cookie": "remember_student_h=SECRETCOOKIE",
                "QmsgKEY": "SECRETQMSG",
                "WXKey": "SCTSECRET",
            }
        ]
    }

    text = auto_add_user.dump_config(config)

    assert "甲" in text and "123456" in text
    assert "SECRETCOOKIE" not in text
    assert "SECRETQMSG" not in text
    assert "SCTSECRET" not in text
    assert "remember_student_h=****" in text


def test_dump_config_keeps_empty_keys_empty():
    text = auto_add_user.dump_config({"students": [{"name": "甲", "cookie": "", "QmsgKEY": "", "WXKey": ""}]})

    assert '"cookie": ""' in text
    assert '"WXKey": ""' in text
