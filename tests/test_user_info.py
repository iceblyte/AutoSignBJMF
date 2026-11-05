import utils.user_info as user_info
from conftest import FakeResponse


class Recorder:
    def __init__(self, responses):
        self.responses = responses
        self.calls = []

    def get(self, url, headers=None, timeout=None):
        self.calls.append({"url": url, "headers": headers or {}, "timeout": timeout})
        for needle, response in self.responses:
            if needle in url:
                return response
        return FakeResponse(text="", status_code=404, url=url)


MY_PAGE = (
    "<html><script>var gconfig={role:'student',type:'fdy',cname:'院系',"
    "uname:'张三',uid:1234 };</script></html>"
)
CLASS_PAGE = (
    '<html><script>var gconfig={role:'
    "'student',type:'fdy',cname:'院系',uname:'张三'};</script>"
    '<a href="/student/course/123456">计算机一班</a>'
    '<div class="card">班级码 AB12CD</div></html>'
)


def test_extracts_remember_student_cookie(monkeypatch):
    recorder = Recorder([("student/my", FakeResponse(text=MY_PAGE)), ("k8n.cn/student", FakeResponse(text=CLASS_PAGE))])
    monkeypatch.setattr(user_info, "requests", recorder)

    student = {"cookie": "remember_student_hash=TOKENVALUE; other=x"}
    user_info.get_user_and_class_info(student)

    sent = recorder.calls[0]["headers"]["Cookie"]
    assert sent == "remember_student_hash=TOKENVALUE"


def test_falls_back_to_full_cookie_for_non_standard_session(monkeypatch):
    """README 明确说 Cookie 可能以 PHPSESSID 开头，此时必须整串发送而不是直接失败。"""
    recorder = Recorder([("student/my", FakeResponse(text=MY_PAGE)), ("k8n.cn/student", FakeResponse(text=CLASS_PAGE))])
    monkeypatch.setattr(user_info, "requests", recorder)

    user_info.get_user_and_class_info({"cookie": "PHPSESSID=abc123; remember_other=1"})

    assert recorder.calls[0]["headers"]["Cookie"] == "PHPSESSID=abc123; remember_other=1"
    name, _ = user_info.get_user_and_class_info({"cookie": "PHPSESSID=abc123"})
    assert name["name"] == "张三"


def test_parses_user_and_class(monkeypatch):
    recorder = Recorder([("student/my", FakeResponse(text=MY_PAGE)), ("k8n.cn/student", FakeResponse(text=CLASS_PAGE))])
    monkeypatch.setattr(user_info, "requests", recorder)

    info_user, info_class = user_info.get_user_and_class_info({"cookie": "remember_student_hash=TOKENVALUE"})

    assert info_user["name"] == "张三"
    assert info_class["class_id"] == "123456"
    assert info_class["class_name"] == "计算机一班"
    assert info_class["class_code"] == "AB12CD"


def test_class_id_from_gconfig_when_present(monkeypatch):
    page = (
        "<html><script>var gconfig={role:'student',type:'fdy',"
        "cname:'院系',id:'777777',uname:'张三'};</script></html>"
    )
    recorder = Recorder([("student/my", FakeResponse(text=MY_PAGE)), ("k8n.cn/student", FakeResponse(text=page))])
    monkeypatch.setattr(user_info, "requests", recorder)

    _, info_class = user_info.get_user_and_class_info({"cookie": "remember_student_hash=TOKENVALUE"})

    assert info_class["class_id"] == "777777"


def test_failed_user_page_reports_not_found(monkeypatch):
    recorder = Recorder(
        [
            ("student/my", FakeResponse(text="", status_code=404)),
            ("k8n.cn/student", FakeResponse(text=CLASS_PAGE)),
        ]
    )
    monkeypatch.setattr(user_info, "requests", recorder)

    info_user, _ = user_info.get_user_and_class_info({"cookie": "remember_student_hash=TOKENVALUE"})

    assert info_user.get("name", "未找到") == "未找到"
