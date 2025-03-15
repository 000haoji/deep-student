#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
启动脚本 - 同时启动主程序和RAG2025服务
"""

import subprocess
import os
import sys
import time
import webbrowser
import signal

def start_services():
    """启动主程序和RAG2025服务"""
    print("正在启动服务...")
    
    # 启动主程序
    main_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "主程序")
    main_script = os.path.join(main_dir, "app.py")
    main_process = subprocess.Popen([sys.executable, main_script], 
                                   cwd=main_dir)
    print(f"→ 主程序已启动 (PID: {main_process.pid})")
    
    # 启动RAG2025
    rag_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "RAG2025")
    rag_script = os.path.join(rag_dir, "app.py")
    rag_process = subprocess.Popen([sys.executable, rag_script], 
                                  cwd=rag_dir)
    print(f"→ RAG2025已启动 (PID: {rag_process.pid})")
    
    print("\n服务访问地址:")
    print("→ 主程序: http://localhost:5001")
    print("→ RAG2025: http://localhost:5000")
    
    # 询问是否自动打开浏览器
    try:
        user_input = input("\n是否自动打开主程序界面？(y/n): ").strip().lower()
        if user_input == 'y':
            # 等待服务启动
            time.sleep(2)
            print("正在打开浏览器...")
            webbrowser.open("http://localhost:5001")
    except KeyboardInterrupt:
        pass
    
    print("\n按 Ctrl+C 停止所有服务...")
    
    try:
        # 等待主进程结束
        main_process.wait()
    except KeyboardInterrupt:
        print("\n正在停止服务...")
        # 优雅地关闭所有进程
        for process in [main_process, rag_process]:
            if process.poll() is None:  # 进程仍在运行
                try:
                    # 尝试优雅关闭
                    if os.name == 'nt':  # Windows
                        process.send_signal(signal.CTRL_C_EVENT)
                    else:  # Linux/Mac
                        process.send_signal(signal.SIGINT)
                    # 给进程一些时间来优雅关闭
                    time.sleep(2)
                    # 如果仍在运行，则强制终止
                    if process.poll() is None:
                        process.terminate()
                except Exception as e:
                    print(f"关闭进程时出错: {e}")
        
        print("所有服务已停止")

if __name__ == "__main__":
    start_services()
