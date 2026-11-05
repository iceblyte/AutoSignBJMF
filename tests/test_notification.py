import re

import utils.notification as notification
from conftest import FakeResponse


class Recorder:
    def __init__(self, status_code=200):
        self.calls = []
        self.status_code = status_code

    def post(self, url, data=None, timeout=None):
        self.calls.append({"url": url, "data": data, "timeout": timeout})
        return FakeResponse(text="ok", status_code=self.status_code, url=url)


def test_qq_skips_when_key_empty(monkeypatch, student):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.sendQQmessage("", msg="x")
    notification.sendQQmessage("   ", msg="x")
    assert recorder.calls == []


def test_qq_sends_prefixed_message(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.sendQQmessage("QKEY", msg="签到成功了")

    call = recorder.calls[0]
    assert call["url"].endswith("/send/QKEY")
    assert re.match(r"^\d{2}:\d{2}:\d{2}  签到成功了$", call["data"]["msg"])
    assert call["timeout"] == 10


def test_qq_default_message_when_none(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.sendQQmessage("QKEY")

    assert recorder.calls[0]["data"]["msg"].endswith("签到成功！")


def test_wx_skips_when_key_empty(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.wx_send("")
    notification.wx_send(None)
    assert recorder.calls == []


def test_wx_sends_to_serverchan(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.wx_send("SCTtest", text="签到成功了", desp="详情")

    call = recorder.calls[0]
    assert call["url"] == "https://sctapi.ftqq.com/SCTtest.send"
    assert call["data"]["desp"] == "详情"
    assert call["data"]["text"].endswith("签到成功了")
    assert call["timeout"] == 10


def test_summary_skips_without_key(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.send_summary_notification("", [("a", "success")])
    assert recorder.calls == []


def test_summary_all_success(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.send_summary_notification("SCTtest", [("甲", "success"), ("乙", "already_signed")])

    call = recorder.calls[0]
    assert call["data"]["text"] == "全部签到成功"
    assert "甲成功" in call["data"]["desp"]
    assert "乙成功" in call["data"]["desp"]
    assert "失败" not in call["data"]["desp"]


def test_summary_groups_failures(monkeypatch):
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.send_summary_notification(
        "SCTtest",
        [("甲", "success"), ("乙", "no_sign_in"), ("丙", "skip")],
    )

    desp = recorder.calls[0]["data"]["desp"]
    assert recorder.calls[0]["data"]["text"] == "部分失败"
    assert "甲成功" in desp
    assert "乙失败" in desp and "丙失败" in desp


def test_summary_request_has_timeout(monkeypatch):
    """汇总通知必须带超时，否则 GitHub Actions 上会挂住整个 job。"""
    recorder = Recorder()
    monkeypatch.setattr(notification, "requests", recorder)

    notification.send_summary_notification("SCTtest", [("甲", "success")])

    assert recorder.calls[0]["timeout"] == 10
