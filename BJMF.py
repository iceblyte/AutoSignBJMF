r'''
Description  : 
Author       : Zheng Hongbin
Date         : 2026-03-20 09:57:33
LastEditTime : 2026-03-20 10:28:11
LastEditors  : Zheng Hongbin
FilePath     : \BJMF\BJMF.py
'''
import sys
import time
import os
from utils import (
    load_config, 
    save_config, 
    get_students,
    get_user_and_class_info,
    sendQQmessage,
    wx_send,
    send_summary_notification,
    Task
)

#设置代理；网络连接错误时尝试
# proxies = {
#     "http": "http://127.0.0.1:7890",
#     "https": "http://127.0.0.1:7890"
# }

if __name__ == "__main__":
    # 默认按失败处理：只有确认「至少有一个学生不是硬失败」才置 0，
    # 这样 Actions 的重试逻辑和退出码才真正有意义（原来恒为 0，重试永远不会触发）。
    exit_code = 1
    try:
        # 获取脚本所在目录的绝对路径
        script_dir = os.path.dirname(os.path.abspath(__file__))
        # 构建data.json的绝对路径
        config_path = os.path.join(script_dir, 'data.json')
        
        # 遍历所有学生，进行签到
        # 先读取原始配置文件用于更新
        original_config = load_config(config_path)
        students = original_config.get('students', [])
        
        # 收集所有签到结果
        attendance_results = []
        
        for i, student in enumerate(students):
            # 如果class字段为空，则先获取用户和班级信息来填充
            if not student.get('class') or student['class'].strip() == "":
                print(f"用户 {student['name']} 的class字段为空，正在获取班级信息...")
                user_info, class_info = get_user_and_class_info(student)
                
                # 检查是否成功获取了用户信息，如果没有则跳过该用户
                user_name = user_info.get('name', '未找到')
                if user_name == "未找到":
                    print(f"无法获取用户信息，可能是cookie过期或无效，跳过用户 {student['name']} 的签到任务")
                    attendance_results.append((student['name'], 'skip'))
                    print("===============================================")
                    print("===============================================\n\n\n")
                    continue
                
                # 获取从网页获取的班级ID
                class_id = class_info.get('class_id')
                if class_id and class_id != "未找到":
                    # 更新内存中的学生配置
                    student['class'] = class_id
                    # 更新原始配置数据
                    original_config['students'][i]['class'] = class_id
                    print(f"已为用户 {student['name']} 设置class字段为: {class_id}")
                else:
                    print(f"无法获取用户 {student['name']} 的班级ID")
                    attendance_results.append((student['name'], 'skip'))
                    print("===============================================")
                    print("===============================================\n\n\n")
                    continue
            
            # 执行签到任务并收集结果
            result = Task(student)
            if result:
                attendance_results.append(result)
            print("===============================================")
            print("===============================================\n\n\n")
        
        # 保存更新后的配置到文件
        save_config(original_config, config_path)
        
        # 发送汇总通知
        # 从学生列表中获取WXKey（使用第一个非空的WXKey）
        wx_key = ''
        for student in students:
            if student.get('WXKey') and student['WXKey'].strip():
                wx_key = student['WXKey']
                break
        if wx_key and wx_key.strip():
            send_summary_notification(wx_key, attendance_results)
        else:
            print("未配置WXKey，跳过汇总通知")

        # skip / error 属于需要人工介入的硬失败（多为 Cookie 失效或网络异常）；
        # no_sign_in、not_started 只是当天没有可签到项，不算故障。
        hard_failures = [name for name, status in attendance_results if status in ("skip", "error")]
        exit_code = 1 if attendance_results and len(hard_failures) == len(attendance_results) else 0
    except KeyboardInterrupt:
        print("程序被手动中断")
        exit_code = 130
    except Exception as error:
        print(f"执行签到时发生未预期错误: {error}")
        import traceback
        traceback.print_exc()
        exit_code = 1
    finally:
        print(f"程序执行完毕，退出码 {exit_code}")
        time.sleep(3)
        sys.exit(exit_code)
