"""扫码登录窗口：显示二维码、轮询状态、成功后回调 Cookie 与班级信息。"""

import io
import queue
import threading
import time
import tkinter as tk
from tkinter import ttk

from PIL import Image, ImageTk

from desktop.qr_client import QRLoginClient

QR_SIZE = 260
POLL_INTERVAL_MS = 300
SESSION_TIMEOUT_SECONDS = 150


class QRLoginDialog(tk.Toplevel):
    def __init__(self, master, on_success):
        super().__init__(master)
        self.title("微信扫码登录")
        self.resizable(False, False)
        self.on_success = on_success
        self._queue = queue.Queue()
        self._photo = None
        self._closed = False

        self._build()
        self.transient(master)
        self.protocol("WM_DELETE_WINDOW", self._close)
        try:
            self.grab_set()
        except tk.TclError:
            pass

        threading.Thread(target=self._fetch_qr, daemon=True).start()
        self.after(POLL_INTERVAL_MS, self._drain)

    # ---------- 界面 ----------

    def _build(self):
        body = ttk.Frame(self, padding=16)
        body.pack(fill="both", expand=True)

        self.status = ttk.Label(body, text="正在获取二维码…", width=36, anchor="center")
        self.status.pack(pady=(0, 10))

        self.image_label = ttk.Label(body, width=36, anchor="center")
        self.image_label.pack()

        self.hint = ttk.Label(
            body,
            text="用微信扫一扫，确认后会自动填入 Cookie、姓名和班级",
            wraplength=QR_SIZE,
            justify="center",
            foreground="#666666",
        )
        self.hint.pack(pady=(10, 12))

        ttk.Button(body, text="取消", command=self._close).pack()

    # ---------- 网络线程 ----------

    def _fetch_qr(self):
        try:
            client = QRLoginClient()
            image_bytes = client.create()
        except Exception as error:
            self._queue.put(("error", f"获取二维码失败：{error}"))
            return

        self._queue.put(("qr", image_bytes))
        threading.Thread(target=self._poll_loop, args=(client,), daemon=True).start()

    def _poll_loop(self, client):
        deadline = time.time() + SESSION_TIMEOUT_SECONDS
        while not self._closed and time.time() < deadline:
            try:
                confirmed, redirect_url = client.poll()
            except Exception:
                confirmed, redirect_url = False, None

            if confirmed and redirect_url:
                try:
                    cookie = client.exchange_cookie(redirect_url)
                    user_info, class_info = client.fetch_profile(cookie)
                except Exception as error:
                    self._queue.put(("error", f"换取登录态失败：{error}"))
                    return
                self._queue.put(("success", cookie, user_info, class_info))
                return

            time.sleep(1)

        self._queue.put(("expired", "二维码已过期，请重新打开扫码窗口"))

    # ---------- 主线程 ----------

    def _drain(self):
        if self._closed:
            return
        try:
            while True:
                event = self._queue.get_nowait()
                self._handle(event)
        except queue.Empty:
            pass
        if not self._closed:
            self.after(POLL_INTERVAL_MS, self._drain)

    def _handle(self, event):
        kind = event[0]

        if kind == "qr":
            self._show_image(event[1])
            self.status.configure(text="请用微信扫描二维码")
            return

        if kind == "success":
            _, cookie, user_info, class_info = event
            self.status.configure(text="登录成功")
            try:
                self.on_success(cookie, user_info, class_info)
            finally:
                self._close()
            return

        if kind == "error":
            self.status.configure(text=event[1])
            self.hint.configure(text="关闭本窗口后可以重新扫码")
            return

        if kind == "expired":
            self.status.configure(text=event[1])
            self.hint.configure(text="关闭本窗口后可以重新扫码")

    def _show_image(self, image_bytes):
        try:
            image = Image.open(io.BytesIO(image_bytes))
            image = image.convert("RGB").resize((QR_SIZE, QR_SIZE), Image.LANCZOS)
            self._photo = ImageTk.PhotoImage(image)
        except Exception as error:
            self.status.configure(text=f"二维码图片解析失败：{error}")
            return
        self.image_label.configure(image=self._photo)

    def _close(self):
        self._closed = True
        try:
            self.grab_release()
        except tk.TclError:
            pass
        self.destroy()
