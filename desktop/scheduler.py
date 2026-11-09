"""后台常驻调度：按本地时间到点执行，失败按配置重试。

界面只负责 start/stop/pause/resume，真正的等待与重试都在这个线程里，
这样关掉窗口缩到托盘后依然会继续跑。
"""

import threading
import traceback
from datetime import datetime, timedelta

from utils.timepoints import next_fire


class DailyScheduler:
    """每天在若干时间点执行 run_once，失败后按间隔重试。"""

    def __init__(
        self,
        run_once,
        times_provider,
        *,
        retry_attempts=3,
        retry_delay_seconds=300,
        now_fn=datetime.now,
        wait_fn=None,
        chunk_seconds=60,
        on_event=None,
    ):
        """
        Args:
            run_once: 无参可调用对象，返回退出码（0 视为成功）
            times_provider: 无参可调用对象，返回 ["21:31:00", ...]
            retry_attempts: 单次到点最多执行几次（含首次）
            retry_delay_seconds: 两次重试之间的等待秒数
            now_fn: 取当前本地时间的函数，测试时替换成假时钟
            wait_fn: 等待函数，返回 False 表示被中断；默认用 Event.wait
            chunk_seconds: 长等待的切片长度，保证暂停/退出能及时响应
            on_event: 状态回调，签名 (event_name, payload)，在调度线程里调用
        """
        self._run_once = run_once
        self._times = times_provider
        self._attempts = max(1, int(retry_attempts))
        self._retry_delay = max(0, int(retry_delay_seconds))
        self._now = now_fn
        self._wait_fn = wait_fn
        self._chunk = max(1, int(chunk_seconds))
        self._on_event = on_event

        self._stop = threading.Event()
        self._paused = threading.Event()
        # 用可重入锁：run_now() 持锁后再进 _execute() 不会自锁
        self._run_lock = threading.RLock()
        self._thread = None

        self.next_run_at = None
        self.last_exit_code = None
        self.last_error = None

    # ---------- 生命周期 ----------

    @property
    def running(self):
        return self._thread is not None and self._thread.is_alive()

    @property
    def paused(self):
        return self._paused.is_set()

    def start(self):
        if self.running:
            return
        self._stop.clear()
        self._paused.clear()
        self._thread = threading.Thread(target=self._loop, name="bjmf-scheduler", daemon=True)
        self._thread.start()

    def stop(self, timeout=5):
        self._stop.set()
        thread = self._thread
        if thread is not None and thread is not threading.current_thread():
            thread.join(timeout)
        self._thread = None
        self.next_run_at = None

    def pause(self):
        self._paused.set()
        self._emit("paused", {})

    def resume(self):
        self._paused.clear()
        self._emit("resumed", {})

    # ---------- 立即执行 ----------

    def run_now(self):
        """在当前线程同步跑一次（不重试），返回退出码。"""
        with self._run_lock:
            return self._execute()

    # ---------- 内部实现 ----------

    def _emit(self, name, payload):
        if self._on_event is None:
            return
        try:
            self._on_event(name, payload)
        except Exception:
            # 回调出错不能影响调度线程
            traceback.print_exc()

    def _sleep(self, seconds):
        if self._wait_fn is not None:
            return bool(self._wait_fn(seconds))
        return not self._stop.wait(seconds)

    def _wait_until(self, target):
        while not self._stop.is_set():
            remaining = (target - self._now()).total_seconds()
            if remaining <= 0:
                return True
            if not self._sleep(min(remaining, self._chunk)):
                return False
        return False

    def _loop(self):
        while not self._stop.is_set():
            try:
                times = self._times() or []
            except Exception as error:
                self._emit("error", {"message": f"读取时间点失败: {error}"})
                times = []

            if not times:
                if not self._sleep(self._chunk):
                    break
                continue

            try:
                target = next_fire(times, self._now())
            except Exception as error:
                self._emit("error", {"message": f"计算下次执行时间失败: {error}"})
                if not self._sleep(self._chunk):
                    break
                continue

            self.next_run_at = target
            self._emit("next_run", {"at": target})

            if not self._wait_until(target):
                break
            if self._stop.is_set():
                break
            if self._paused.is_set():
                self._emit("skipped", {"at": target, "reason": "paused"})
                continue

            self._run_with_retry()

        self.next_run_at = None
        self._emit("stopped", {})

    def _run_with_retry(self):
        for attempt in range(1, self._attempts + 1):
            self._emit("run_start", {"attempt": attempt, "total": self._attempts})
            code = self._execute()
            self._emit("run_done", {"attempt": attempt, "total": self._attempts, "code": code})

            if code == 0 or attempt >= self._attempts:
                return code
            if self._retry_delay and not self._wait_until(self._now() + timedelta(seconds=self._retry_delay)):
                return code
        return self.last_exit_code

    def _execute(self):
        with self._run_lock:
            try:
                code = self._run_once()
            except KeyboardInterrupt:
                raise
            except Exception as error:
                self.last_error = f"{type(error).__name__}: {error}"
                self.last_exit_code = 1
                self._emit("error", {"message": self.last_error, "detail": traceback.format_exc()})
                return 1
            self.last_exit_code = 0 if code is None else int(code)
            if self.last_exit_code == 0:
                self.last_error = None
            return self.last_exit_code
