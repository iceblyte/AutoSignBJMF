"""微信扫码登录（桌面端版）。

和 `auto_add_user.py` 的流程一致（拉二维码 → 轮询 → 用跳转地址换 Cookie），
但把图片交给界面显示，且**不打印 Cookie 明文**。
"""

import requests
from bs4 import BeautifulSoup

from utils.user_info import get_user_and_class_info

BASE_URL = "https://bjmf.k8n.cn/weixin/qrlogin/student"
UIDLOGIN_URL = "https://bj.k8n.cn/student/uidlogin?"

MOBILE_UA = (
    "Mozilla/5.0 (Linux; Android 10; SM-G981B) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/80.0.3987.162 Mobile Safari/537.36 "
    "MicroMessenger/7.0.10.1580(0x27000A50) Process/tools NetType/WIFI Language/zh_CN ABI/arm64"
)


def pick_login_cookie(cookies):
    """从 Cookie 字典里挑出登录态。

    - 丢掉无用的 `s`
    - 优先 `remember_student_*`（值里带站点哈希和用户票据）
    - 没有就退回第一个，保证 PHPSESSID 这类会话也能用
    """
    remaining = {key: value for key, value in cookies.items() if key != "s"}
    for key, value in remaining.items():
        if key.startswith("remember_student_"):
            return f"{key}={value}"
    for key, value in remaining.items():
        return f"{key}={value}"
    raise ValueError("未获取到有效 cookie")


def build_uidlogin_url(redirect_url):
    """把扫码成功后的跳转地址拼成换取 Cookie 的地址。"""
    if not redirect_url or "?" not in redirect_url:
        raise ValueError("登录跳转地址缺少参数，无法换取 Cookie")
    return UIDLOGIN_URL + redirect_url.split("?", 1)[1]


def extract_qr_url(html):
    """从登录页里找出二维码图片地址。"""
    soup = BeautifulSoup(html, "html.parser")
    container = soup.find("div", id="qrcode")
    if container:
        image = container.find("img")
        if image and image.get("src"):
            return image["src"]
    for image in soup.find_all("img"):
        src = image.get("src") or ""
        if "ticket=" in src:
            return src
    raise ValueError("登录页里没找到二维码图片，可能接口已变更")


class QRLoginClient:
    """一轮扫码会话。一个实例对应一次二维码。"""

    def __init__(self, timeout=15, session=None):
        self.timeout = timeout
        self.session = session or self._new_session()

    @staticmethod
    def _new_session():
        session = requests.Session()
        session.headers.update(
            {
                "User-Agent": MOBILE_UA,
                "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
                "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
                "Referer": "https://wx.qq.com/",
                "X-Requested-With": "XMLHttpRequest",
            }
        )
        return session

    def create(self):
        """返回二维码图片的二进制内容（PNG）。"""
        html = self.session.get(BASE_URL, timeout=self.timeout).text
        image_url = extract_qr_url(html)
        response = self.session.get(image_url, timeout=self.timeout)
        response.raise_for_status()
        return response.content

    def poll(self):
        """查询扫码状态，返回 (是否已确认, 跳转地址)。"""
        response = self.session.get(f"{BASE_URL}?op=checklogin", timeout=self.timeout)
        if response.status_code != 200:
            return False, None
        try:
            data = response.json()
        except ValueError:
            return False, None
        if data.get("status") and data.get("url"):
            return True, data["url"]
        return False, None

    def exchange_cookie(self, redirect_url):
        """访问跳转地址，取回 remember_student_* 登录态。"""
        url = build_uidlogin_url(redirect_url)
        response = self.session.get(url, allow_redirects=False, timeout=self.timeout)
        response.raise_for_status()
        return pick_login_cookie(self.session.cookies.get_dict())

    def fetch_profile(self, cookie):
        """用新 Cookie 拉一次姓名与班级，顺便验证 Cookie 是否可用。"""
        return get_user_and_class_info({"cookie": cookie})
