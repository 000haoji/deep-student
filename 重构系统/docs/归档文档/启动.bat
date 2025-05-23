@echo off
chcp 65001 >nul
title 错题管理系统启动器

echo ========================================
echo  错题管理系统 2.0 - 启动中...
echo ========================================
echo.

echo [1/2] 启动后端服务...
cd /d "H:\Deep-Students\重构系统\backend"
start "后端服务 - 错题管理系统" cmd /k "venv\Scripts\activate && python -m uvicorn main:app --reload --host 127.0.0.1 --port 8000"

echo [2/2] 启动前端服务...
cd /d "H:\Deep-Students\重构系统\frontend"
start "前端服务 - 错题管理系统" cmd /k "npm run dev"

echo.
echo ========================================
echo  服务启动完成！
echo  后端地址: http://localhost:8000
echo  前端地址: http://localhost:3000
echo  
echo  注意：如果3000端口被占用，前端会自动使用3001端口
echo ========================================
echo.
pause 