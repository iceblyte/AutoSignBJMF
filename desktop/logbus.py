"""把 print 输出接到界面日志上。

PyInstaller 用 --noconsole 打包后 sys.stdout 是 None，
直接 print 会抛 AttributeError；这里提供一个安全的下水道。
"""

_sink = None


def set_sink(writer):
    global _sink
    _sink = writer


def write(text):
    if _sink is None:
        return
    try:
        _sink(text)
    except Exception:
        pass


class StreamProxy:
    """够用的 sys.stdout / sys.stderr 替身。"""

    def __init__(self, prefix=""):
        self._prefix = prefix
        self._buffer = ""

    def write(self, text):
        if not isinstance(text, str):
            text = str(text)
        self._buffer += text
        while "\n" in self._buffer:
            line, self._buffer = self._buffer.split("\n", 1)
            if line.strip():
                write(f"{self._prefix}{line}")
        return len(text)

    def flush(self):
        if self._buffer.strip():
            write(f"{self._prefix}{self._buffer}")
        self._buffer = ""

    def isatty(self):
        return False
