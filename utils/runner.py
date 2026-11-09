"""单次签到执行流程。

从 BJMF.py 抽出来，让命令行和桌面端（exe 内）复用同一套逻辑；
不再靠 `sys.exit` 结束进程，而是返回退出码，方便 GUI 在后台线程里调用。
"""

import os
import traceback

from . import (
    Task,
    get_user_and_class_info,
    load_config,
    save_config,
    send_summary_notification,
)

HARD_FAILURE_STATUS = ("skip", "error")


def default_config_path():
    """命令行版默认读取脚本同级的 data.json。"""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(root, "data.json")


def run_once(config_path=None):
    """跑一轮所有学生的签到。

    Args:
        config_path: data.json 路径；默认取项目根目录下的 data.json

    Returns:
        int: 退出码。0 = 至少有一个学生不是硬失败（或无学生），1 = 全部硬失败。
             KeyboardInterrupt 会向上抛出，由调用方决定怎么处理。
    """
    config_path = config_path or default_config_path()
    try:
        original_config = load_config(config_path) or {}
        students = original_config.get("students", [])
        results = []

        for index, student in enumerate(students):
            # class 为空时先探测真实班级，能补就补，补不到就跳过这个人
            if not student.get("class") or str(student["class"]).strip() == "":
                print(f"用户 {student.get('name', '未知')} 的class字段为空，正在获取班级信息...")
                user_info, class_info = get_user_and_class_info(student)

                if user_info.get("name", "未找到") == "未找到":
                    print(
                        f"无法获取用户信息，可能是cookie过期或无效，跳过用户 "
                        f"{student.get('name', '未知')} 的签到任务"
                    )
                    results.append((student.get("name", "未知"), "skip"))
                    print("===============================================")
                    print("===============================================\n\n\n")
                    continue

                class_id = class_info.get("class_id")
                if class_id and class_id != "未找到":
                    student["class"] = class_id
                    original_config["students"][index]["class"] = class_id
                    print(f"已为用户 {student.get('name')} 设置class字段为: {class_id}")
                else:
                    print(f"无法获取用户 {student.get('name')} 的班级ID")
                    results.append((student.get("name", "未知"), "skip"))
                    print("===============================================")
                    print("===============================================\n\n\n")
                    continue

            result = Task(student)
            if result:
                results.append(result)
            print("===============================================")
            print("===============================================\n\n\n")

        save_config(original_config, config_path)

        # 汇总通知用第一个非空的 WXKey（顶层 WXKey 字段已废弃）
        wx_key = ""
        for student in students:
            if student.get("WXKey") and student["WXKey"].strip():
                wx_key = student["WXKey"]
                break
        if wx_key:
            send_summary_notification(wx_key, results)
        else:
            print("未配置WXKey，跳过汇总通知")

        # skip / error 属于需要人工介入的硬失败；no_sign_in、not_started 只是当天没东西可签
        hard_failures = [name for name, status in results if status in HARD_FAILURE_STATUS]
        return 1 if results and len(hard_failures) == len(results) else 0
    except KeyboardInterrupt:
        raise
    except Exception as error:
        print(f"执行签到时发生未预期错误: {error}")
        traceback.print_exc()
        return 1
