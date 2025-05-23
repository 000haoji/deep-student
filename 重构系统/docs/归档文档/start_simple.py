"""
Windows 平台简单启动脚本
直接启动前后端服务，不检查依赖
"""
import subprocess
import os
import sys
import threading
import time

def start_backend():
    """启动后端服务"""
    print("启动后端服务...")
    backend_dir = os.path.join(os.path.dirname(__file__), "backend")
    os.chdir(backend_dir)
    
    # 激活虚拟环境并启动
    activate_script = os.path.join("venv", "Scripts", "activate.bat")
    cmd = f'cmd /c "{activate_script} && python -m uvicorn main:app --reload --host 127.0.0.1 --port 8000"'
    
    process = subprocess.Popen(cmd, shell=True)
    return process

def start_frontend():
    """启动前端服务"""
    print("启动前端服务...")
    frontend_dir = os.path.join(os.path.dirname(__file__), "frontend")
    os.chdir(frontend_dir)
    
    cmd = 'npm run dev'
    process = subprocess.Popen(cmd, shell=True)
    return process

def main():
    print("="*60)
    print(" 错题管理系统 2.0 - 简单启动脚本")
    print("="*60)
    print()
    
    # 保存原始目录
    original_dir = os.getcwd()
    
    try:
        # 启动后端
        backend_process = start_backend()
        
        # 等待后端启动
        print("等待后端启动...")
        time.sleep(5)
        
        # 切回原始目录
        os.chdir(original_dir)
        
        # 启动前端
        frontend_process = start_frontend()
        
        print()
        print("服务启动完成！")
        print("后端地址: http://localhost:8000")
        print("前端地址: http://localhost:3000 或 http://localhost:3001")
        print()
        print("按 Ctrl+C 停止所有服务")
        
        # 等待进程
        backend_process.wait()
        frontend_process.wait()
        
    except KeyboardInterrupt:
        print("\n停止所有服务...")
        backend_process.terminate()
        frontend_process.terminate()
        sys.exit(0)
    except Exception as e:
        print(f"启动失败: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main() 