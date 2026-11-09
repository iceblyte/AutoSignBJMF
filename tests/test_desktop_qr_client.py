"""扫码登录里可以脱离网络单独验证的部分。"""

import pytest

from desktop.qr_client import build_uidlogin_url, extract_qr_url, pick_login_cookie


def test_prefers_remember_student_cookie():
    cookies = {"s": "session-id", "PHPSESSID": "abc", "remember_student_hash": "uid%7Ctoken%7C"}

    assert pick_login_cookie(cookies) == "remember_student_hash=uid%7Ctoken%7C"


def test_drops_useless_s_cookie():
    assert pick_login_cookie({"s": "x", "PHPSESSID": "abc"}) == "PHPSESSID=abc"


def test_raises_when_no_cookie():
    with pytest.raises(ValueError):
        pick_login_cookie({})
    with pytest.raises(ValueError):
        pick_login_cookie({"s": "only-s"})


def test_build_uidlogin_url_keeps_query():
    url = build_uidlogin_url("https://bjmf.k8n.cn/weixin/qrlogin/student?uid=1&ticket=abc")

    assert url == "https://bj.k8n.cn/student/uidlogin?uid=1&ticket=abc"


def test_build_uidlogin_url_rejects_missing_query():
    with pytest.raises(ValueError):
        build_uidlogin_url("https://bjmf.k8n.cn/weixin/qrlogin/student")
    with pytest.raises(ValueError):
        build_uidlogin_url("")


def test_extract_qr_url_from_qrcode_div():
    html = '<div id="qrcode"><img src="https://example.com/qr?ticket=abc"></div>'

    assert extract_qr_url(html) == "https://example.com/qr?ticket=abc"


def test_extract_qr_url_falls_back_to_ticket_image():
    html = '<div class="other"><img src="https://example.com/qr?ticket=xyz"></div>'

    assert extract_qr_url(html) == "https://example.com/qr?ticket=xyz"


def test_extract_qr_url_raises_when_absent():
    with pytest.raises(ValueError):
        extract_qr_url("<html><body>没有二维码</body></html>")
