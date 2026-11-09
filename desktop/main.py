"""桌面端入口。

- 直接运行：`python desktop/main.py` 或 `python -m desktop.main`
- 打包后：`AutoSignBJMF.exe`
- 自检：`AutoSignBJMF.exe --self-check`（不开窗口，检查依赖与数据目录）
"""

import sys

from desktop.logbus import StreamProxy, set_sink

# 必须在其它模块之前：--noconsole 打包后 stdout 为 None，print 会抛 AttributeError
if sys.stdout is None:
    sys.stdout = StreamProxy()
if sys.stderr is None:
    sys.stderr = StreamProxy("[stderr] ")


def _acquire_single_instance():
    """同一时间只允许一个实例在跑，避免两个调度器重复签到。"""
    if sys.platform != "win32":
        return True
    try:
        import ctypes

        kernel32 = ctypes.windll.kernel32
        handle = kernel32.CreateMutexW(None, False, "Local\\AutoSignBJMF_SingleInstance")
        already_exists = kernel32.GetLastError() == 183  # ERROR_ALREADY_EXISTS
        if already_exists:
            return None
        return handle
    except Exception:
        return True


def _enable_dpi_awareness():
    if sys.platform != "win32":
        return
    try:
        import ctypes

        ctypes.windll.shcore.SetProcessDpiAwareness(1)
    except Exception:
        pass


def _set_window_icon(root):
    try:
        from PIL import ImageTk

        from desktop.tray import _make_icon_image

        root._icon_photo = ImageTk.PhotoImage(_make_icon_image(64))
        root.iconphoto(True, root._icon_photo)
    except Exception:
        pass


def _report(lines):
    """无控制台的 exe 也要能看到自检结果：写到 selfcheck.txt 并尽量 print。"""
    from pathlib import Path

    text = "\n".join(str(line) for line in lines)
    try:
        print(text)
    except Exception:
        pass
    try:
        (Path.cwd() / "selfcheck.txt").write_text(text + "\n", encoding="utf-8")
    except OSError:
        pass


def _self_check():
    import requests

    import desktop.paths as paths

    lines = [
        f"python {sys.version.split()[0]}",
        f"frozen {getattr(sys, 'frozen', False)}",
        f"data_dir {paths.ensure_data_dir()}",
        f"requests {requests.__version__}",
        "self-check ok",
    ]
    _report(lines)
    return 0


def _smoke_check():
    """建一次窗口再销毁，用来验证打包后的 tkinter / pystray 是否齐全。"""
    import tkinter as tk

    from desktop.app import SigninApp

    root = tk.Tk()
    root.withdraw()
    app = SigninApp(root)
    root.update()
    tray_ready = app._tray is not None
    app.destroy()
    root.destroy()
    _report([f"smoke ok", f"tray {'available' if tray_ready else 'unavailable'}"])
    return 0


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if "--self-check" in argv:
        return _self_check()
    if "--smoke" in argv:
        return _smoke_check()

    mutex = _acquire_single_instance()
    if mutex is None:
        import tkinter as tk
        from tkinter import messagebox

        root = tk.Tk()
        root.withdraw()
        messagebox.showinfo("班级魔方自动签到", "程序已经在运行了，请查看右下角托盘图标。")
        root.destroy()
        return 0

    _enable_dpi_awareness()

    import tkinter as tk

    from desktop.app import SigninApp

    root = tk.Tk()
    app = SigninApp(root)
    set_sink(app.post_pipe_output)
    _set_window_icon(root)

    try:
        root.mainloop()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
