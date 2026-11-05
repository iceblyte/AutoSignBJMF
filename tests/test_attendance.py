from conftest import FakeResponse, FakeSession

import utils.attendance as attendance


def _patch_user_info(monkeypatch, name="测试用户", class_id="123456"):
    monkeypatch.setattr(
        attendance,
        "get_user_and_class_info",
        lambda student: ({"name": name}, {"class_id": class_id, "class_name": "测试班级"}),
    )


def _patch_session(monkeypatch, session):
    monkeypatch.setattr(attendance.requests, "Session", lambda: session)


def test_skip_when_cookie_expired(monkeypatch, student):
    _patch_user_info(monkeypatch, name="未找到")
    session = FakeSession()
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "skip")


def test_no_sign_in_writes_debug_html_in_cwd(monkeypatch, student, tmp_path):
    _patch_user_info(monkeypatch)
    monkeypatch.chdir(tmp_path)
    session = FakeSession(
        get_rules=[("punchs", FakeResponse(text="<html>没有签到项</html>", url="u1")),
                   ("daka", FakeResponse(text="<html>没有打卡项</html>", url="u2"))],
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "no_sign_in")
    assert (tmp_path / "debug_html.txt").read_text(encoding="utf-8") == "<html>没有打卡项</html>"


def test_already_signed_via_success_info_block(monkeypatch, student, tmp_path):
    _patch_user_info(monkeypatch)
    monkeypatch.chdir(tmp_path)
    html = '<div class="punch-success-info">已签到 09:00</div>'
    session = FakeSession(get_rules=[("/", FakeResponse(text=html, url="u"))])
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "already_signed")


def test_already_signed_via_status_block(monkeypatch, student, tmp_path):
    _patch_user_info(monkeypatch)
    monkeypatch.chdir(tmp_path)
    html = '<div class="punch-status">已签到</div>'
    session = FakeSession(get_rules=[("/", FakeResponse(text=html, url="u"))])
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "already_signed")


def test_sign_in_success_posts_location(monkeypatch, student):
    _patch_user_info(monkeypatch)
    session = FakeSession(
        get_rules=[("punchs", FakeResponse(text='<a href="/student/punchs/course/123456/777">x</a>', url="u"))],
        post_response=FakeResponse(text='<div id="title">签到成功</div>', status_code=200),
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "success")
    url, payload, timeout = session.post_calls[0]
    assert "/student/punchs/course/123456/777" in url
    assert payload["lat"] == "30.123456"
    assert payload["lng"] == "120.123456"
    assert payload["acc"] == "30"
    assert timeout == 10


def test_already_signed_from_post_title(monkeypatch, student):
    _patch_user_info(monkeypatch)
    session = FakeSession(
        get_rules=[("punchs", FakeResponse(text="punchcard_777", url="u"))],
        post_response=FakeResponse(text='<div id="title">已签到，无需重复</div>', status_code=200),
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "already_signed")


def test_not_started_from_post_title(monkeypatch, student):
    _patch_user_info(monkeypatch)
    session = FakeSession(
        get_rules=[("punchs", FakeResponse(text="punchcard_777", url="u"))],
        post_response=FakeResponse(text='<div id="title">签到未开始</div>', status_code=200),
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "not_started")


def test_error_on_bad_status_code(monkeypatch, student):
    _patch_user_info(monkeypatch)
    session = FakeSession(
        get_rules=[("punchs", FakeResponse(text="punchcard_777", url="u"))],
        post_response=FakeResponse(text="", status_code=500),
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student) == ("测试用户", "error")


def test_probes_both_modules_and_signs_one_item(monkeypatch, student):
    """上游重构后应同时探测 punchs 与 daka 两类模块；命中多项时只提交其中一项。"""
    _patch_user_info(monkeypatch)
    session = FakeSession(
        get_rules=[
            ("punchs", FakeResponse(text="punchcard_111", url="u1")),
            ("daka", FakeResponse(text='<a href="/student/daka/course/123456/222">y</a>', url="u2")),
        ],
        post_response=FakeResponse(text='<div id="title">签到成功</div>', status_code=200),
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student)[1] == "success"
    assert any("/punchs" in call[0] for call in session.get_calls)
    assert any("/daka" in call[0] for call in session.get_calls)
    assert len(session.post_calls) == 1
    posted_url = session.post_calls[0][0]
    assert "/111" in posted_url or "/222" in posted_url


def test_direct_redirect_url_is_detected(monkeypatch, student):
    _patch_user_info(monkeypatch)
    session = FakeSession(
        get_rules=[("punchs", FakeResponse(text="nothing here", url="https://bjmf.k8n.cn/student/punchw/course/123456/333"))],
        post_response=FakeResponse(text='<div id="title">签到成功</div>', status_code=200),
    )
    _patch_session(monkeypatch, session)

    assert attendance.Task(student)[1] == "success"
    assert "/333" in session.post_calls[0][0]


def test_network_exception_returns_error(monkeypatch, student):
    _patch_user_info(monkeypatch)

    class Boom(FakeSession):
        def get(self, url, timeout=None):
            raise attendance.requests.exceptions.ConnectionError("boom")

        def post(self, url, data=None, timeout=None):
            raise attendance.requests.exceptions.ConnectionError("boom")

    _patch_session(monkeypatch, Boom())

    assert attendance.Task(student) == ("测试用户", "error")
