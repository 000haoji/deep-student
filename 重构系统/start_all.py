"""
系统启动脚本
同时启动后端API和前端开发服务器
"""
import subprocess
import sys
import time
import os
from pathlib import Path

def check_requirements():
    """检查系统要求"""
    print("检查系统要求...")
    
    # 检查Python版本
    if sys.version_info < (3, 8):
        print("错误: 需要Python 3.8或更高版本")
        return False
    
    # 检查Node.js
    try:
        subprocess.run(["node", "--version"], capture_output=True, check=True)
    except:
        print("错误: 未安装Node.js，请先安装Node.js")
        return False
    
    print("✓ 系统要求检查通过")
    return True

def install_backend_deps():
    """安装后端依赖"""
    print("\n安装后端依赖...")
    backend_dir = Path(__file__).parent / "backend"
    os.chdir(backend_dir)
    
    # 创建虚拟环境（如果不存在）
    if not (backend_dir / "venv").exists():
        print("创建Python虚拟环境...")
        subprocess.run([sys.executable, "-m", "venv", "venv"])
    
    # 激活虚拟环境并安装依赖
    if sys.platform == "win32":
        pip = str(backend_dir / "venv" / "Scripts" / "pip")
        python = str(backend_dir / "venv" / "Scripts" / "python")
    else:
        pip = str(backend_dir / "venv" / "bin" / "pip")
        python = str(backend_dir / "venv" / "bin" / "python")
    
    print("安装Python依赖...")
    subprocess.run([pip, "install", "-r", "requirements.txt"])
    
    # 额外安装缺失的包
    subprocess.run([pip, "install", "werkzeug", "pyjwt"])
    
    print("✓ 后端依赖安装完成")
    return python

def install_frontend_deps():
    """安装前端依赖"""
    print("\n安装前端依赖...")
    frontend_dir = Path(__file__).parent / "frontend"
    os.chdir(frontend_dir)
    
    print("安装Node.js依赖...")
    subprocess.run(["npm", "install"])
    
    print("✓ 前端依赖安装完成")

def init_database(python_exe):
    """初始化数据库"""
    print("\n初始化数据库...")
    backend_dir = Path(__file__).parent / "backend"
    os.chdir(backend_dir)
    
    # 运行数据库初始化脚本
    subprocess.run([python_exe, "init_db.py"])
    
    print("✓ 数据库初始化完成")

def start_backend(python_exe):
    """启动后端服务"""
    print("\n启动后端服务...")
    backend_dir = Path(__file__).parent / "backend"
    os.chdir(backend_dir)
    
    # 启动FastAPI服务
    cmd = [python_exe, "-m", "uvicorn", "app:app", "--reload", "--host", "0.0.0.0", "--port", "8000"]
    return subprocess.Popen(cmd)

def start_frontend():
    """启动前端服务"""
    print("\n启动前端服务...")
    frontend_dir = Path(__file__).parent / "frontend"
    os.chdir(frontend_dir)
    
    # 启动Vite开发服务器
    cmd = ["npm", "run", "dev"]
    return subprocess.Popen(cmd, shell=True)

def main():
    """主函数"""
    print("="*60)
    print(" 错题管理系统 2.0 - 启动脚本")
    print("="*60)
    
    # 检查系统要求
    if not check_requirements():
        return
    
    try:
        # 安装依赖
        python_exe = install_backend_deps()
        install_frontend_deps()
        
        # 初始化数据库
        init_database(python_exe)
        
        # 启动服务
        backend_process = start_backend(python_exe)
        time.sleep(3)  # 等待后端启动
        
        frontend_process = start_frontend()
        time.sleep(3)  # 等待前端启动
        
        print("\n" + "="*60)
        print(" 系统启动成功！")
        print("="*60)
        print("\n访问地址:")
        print("  前端界面: http://localhost:3000")
        print("  后端API文档: http://localhost:8000/docs")
        print("\n演示账号:")
        print("  用户名: demo_user")
        print("  密码: demo123")
        print("\n按 Ctrl+C 停止所有服务")
        print("="*60)
        
        # 等待用户中断
        try:
            backend_process.wait()
        except KeyboardInterrupt:
            print("\n正在停止服务...")
            backend_process.terminate()
            frontend_process.terminate()
            print("服务已停止")
            
    except Exception as e:
        print(f"\n错误: {e}")
        return

if __name__ == "__main__":
    main() 