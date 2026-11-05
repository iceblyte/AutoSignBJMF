import os
import sys

import pytest

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

WEB_SIGNIN_DIR = os.path.join(ROOT_DIR, "web_signin")
if WEB_SIGNIN_DIR not in sys.path:
    sys.path.insert(0, WEB_SIGNIN_DIR)


class FakeResponse:
    def __init__(self, text="", status_code=200, url=""):
        self.text = text
        self.status_code = status_code
        self.url = url


class FakeSession:
    """记录请求并按 URL 关键字返回预设响应，避免测试触达真实网络。"""

    def __init__(self, get_rules=None, post_response=None, post_rules=None):
        self.get_rules = get_rules or []
        self.post_rules = post_rules or []
        self.post_response = post_response
        self.headers = {}
        self.get_calls = []
        self.post_calls = []

    def _match(self, rules, url):
        for needle, response in rules:
            if needle in url:
                return response
        return None

    def update(self, other):
        self.headers.update(other)

    def get(self, url, timeout=None):
        self.get_calls.append((url, timeout))
        response = self._match(self.get_rules, url)
        return response if response is not None else FakeResponse(text="", status_code=404, url=url)

    def post(self, url, data=None, timeout=None):
        self.post_calls.append((url, data, timeout))
        response = self._match(self.post_rules, url)
        if response is not None:
            return response
        return self.post_response or FakeResponse(text="", status_code=500, url=url)


@pytest.fixture
def student():
    return {
        "name": "测试用户",
        "class": "123456",
        "lat": "30.123456",
        "lng": "120.123456",
        "acc": "30",
        "cookie": "remember_student_0000000000000000000000000000000000000000=123456%7Ctoken%7C",
        "QmsgKEY": "",
        "WXKey": "",
    }
