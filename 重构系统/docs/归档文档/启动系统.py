import subprocess
import os
import time

print("="*50)
print("错题管理系统 2.0 - 启动中...")
print("="*50)

# 启动后端
backend_path = r"H:\Deep-Students\重构系统\backend"
print("\n[1] 启动后端服务...")
backend_cmd = f'start "后端服务" cmd /k "cd /d {backend_path} && venv\\Scripts\\activate && python -m uvicorn main:app --reload --host 127.0.0.1 --port 8000"'
subprocess.run(backend_cmd, shell=True)

# 等待后端启动
print("    等待后端启动...")
time.sleep(5)

# 启动前端
frontend_path = r"H:\Deep-Students\重构系统\frontend"
print("\n[2] 启动前端服务...")
frontend_cmd = f'start "前端服务" cmd /k "cd /d {frontend_path} && npm run dev"'
subprocess.run(frontend_cmd, shell=True)

print("\n" + "="*50)
print("启动完成！")
print("后端: http://localhost:8000")
print("前端: http://localhost:3000")
print("="*50)
print("\n按任意键退出...")
input() 