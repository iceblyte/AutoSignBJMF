"""命令行入口：执行一次签到。

真正的流程在 `utils/runner.py`，桌面端（exe）用同一个函数，
这样"点立即签到"和 `python BJMF.py` 走的是同一套逻辑。
"""

import sys
import time

from utils.runner import default_config_path, run_once


def main(argv=None):
    exit_code = 1
    try:
        exit_code = run_once(default_config_path())
    except KeyboardInterrupt:
        print("程序被手动中断")
        exit_code = 130
    finally:
        print(f"程序执行完毕，退出码 {exit_code}")
        time.sleep(3)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
