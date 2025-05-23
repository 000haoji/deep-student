"""
Windows 平台一键启动脚本
解决 PowerShell 不支持 && 的问题
"""
import subprocess
import os
import sys
import time
import threading
from pathlib import Path

def print_banner():
    """打印启动横幅"""
    print("="*60)
    print(" 错题管理系统 2.0 - Windows 启动脚本")
    print("="*60)
    print()

def check_requirements():
    """检查系统要求"""
    print("检查系统要求...")
    
    # 检查Python版本
    if sys.version_info < (3, 8):
        print("❌ 错误: 需要Python 3.8或更高版本")
        return False
    else:
        print(f"✓ Python {sys.version.split()[0]}")
    
    # 检查Node.js
    try:
        result = subprocess.run(["node", "--version"], capture_output=True, text=True)
        if result.returncode == 0:
            print(f"✓ Node.js {result.stdout.strip()}")
        else:
            raise Exception()
    except:
        print("❌ 错误: 未安装Node.js，请先安装Node.js")
        return False
    
    print("✓ 系统要求检查通过\n")
    return True

def run_backend():
    """启动后端服务"""
    backend_dir = Path(__file__).parent / "backend"
    
    print("启动后端服务...")
    
    # 切换到后端目录
    os.chdir(backend_dir)
    
    # 检查虚拟环境
    venv_path = backend_dir / "venv"
    if not venv_path.exists():
        print("创建Python虚拟环境...")
        subprocess.run([sys.executable, "-m", "venv", "venv"])
    
    # 确定Python路径
    if sys.platform == "win32":
        python = str(backend_dir / "venv" / "Scripts" / "python.exe")
        pip = str(backend_dir / "venv" / "Scripts" / "pip.exe")
    else:
        python = str(backend_dir / "venv" / "bin" / "python")
        pip = str(backend_dir / "venv" / "bin" / "pip")
    
    # 安装必要的依赖（只检查关键包）
    print("检查后端依赖...")
    try:
        subprocess.run([pip, "show", "fastapi"], capture_output=True, check=True)
        subprocess.run([pip, "show", "uvicorn"], capture_output=True, check=True)
        subprocess.run([pip, "show", "jose"], capture_output=True, check=True)
    except:
        print("安装后端依赖...")
        subprocess.run([pip, "install", "-r", "requirements.txt"], check=True)
        subprocess.run([pip, "install", "python-jose[cryptography]"], check=True)
    
    # 启动后端服务
    print("后端服务启动中...")
    cmd = [python, "-m", "uvicorn", "main:app", "--reload", "--host", "127.0.0.1", "--port", "8000"]
    return subprocess.Popen(cmd)

def run_frontend():
    """启动前端服务"""
    frontend_dir = Path(__file__).parent / "frontend"
    
    print("启动前端服务...")
    
    # 切换到前端目录
    os.chdir(frontend_dir)
    
    # 检查node_modules
    if not (frontend_dir / "node_modules").exists():
        print("安装前端依赖...")
        subprocess.run(["npm", "install"], check=True)
    else:
        print("前端依赖已安装")
    
    # 启动前端服务
    print("前端服务启动中...")
    # Windows下需要使用shell=True
    return subprocess.Popen("npm run dev", shell=True)

def check_services():
    """检查服务是否启动成功"""
    import socket
    import time
    
    print("\n检查服务状态...")
    
    # 检查后端
    backend_ok = False
    for i in range(10):
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            result = sock.connect_ex(('127.0.0.1', 8000))
            sock.close()
            if result == 0:
                backend_ok = True
                break
        except:
            pass
        time.sleep(1)
    
    # 检查前端
    frontend_ok = False
    for i in range(10):
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            result = sock.connect_ex(('127.0.0.1', 3000))
            sock.close()
            if result == 0:
                frontend_ok = True
                break
        except:
            pass
        time.sleep(1)
    
    return backend_ok, frontend_ok

def main():
    """主函数"""
    print_banner()
    
    # 检查系统要求
    if not check_requirements():
        input("\n按Enter键退出...")
        return
    
    # 保存当前目录
    root_dir = Path(__file__).parent
    
    try:
        # 启动后端（在新线程中）
        backend_process = run_backend()
        
        # 等待后端启动
        time.sleep(3)
        
        # 切回根目录
        os.chdir(root_dir)
        
        # 启动前端
        frontend_process = run_frontend()
        
        # 等待服务启动
        time.sleep(5)
        
        # 检查服务状态
        backend_ok, frontend_ok = check_services()
        
        print("\n" + "="*60)
        if backend_ok and frontend_ok:
            print(" ✅ 系统启动成功！")
            print("="*60)
            print("\n访问地址:")
            print("  前端界面: http://localhost:3000")
            print("  后端API文档: http://localhost:8000/docs")
            print("\n演示账号:")
            print("  用户名: demo_user")
            print("  密码: demo123")
        else:
            print(" ⚠️  部分服务启动失败")
            if not backend_ok:
                print("  - 后端服务启动失败")
            if not frontend_ok:
                print("  - 前端服务启动失败")
        
        print("\n按 Ctrl+C 停止所有服务")
        print("="*60)
        
        # 等待用户中断
        try:
            # 等待进程结束
            backend_process.wait()
        except KeyboardInterrupt:
            print("\n\n正在停止服务...")
            backend_process.terminate()
            frontend_process.terminate()
            
            # 给进程一些时间正常退出
            time.sleep(2)
            
            # 强制结束
            if backend_process.poll() is None:
                backend_process.kill()
            if frontend_process.poll() is None:
                frontend_process.kill()
                
            print("✓ 服务已停止")
            
    except Exception as e:
        print(f"\n❌ 错误: {e}")
        import traceback
        traceback.print_exc()
        input("\n按Enter键退出...")

if __name__ == "__main__":
    main() 