@echo off
chcp 65001 >nul
echo ========================================
echo  错题管理系统 2.0 - 启动中...
echo ========================================
echo.

echo [1/2] 启动后端服务...
start "后端服务" cmd /c "cd backend && python -m uvicorn main:app --reload --host 0.0.0.0 --port 8000 & pause"

echo [2/2] 等待5秒后启动前端服务...
timeout /t 5 /nobreak > nul
start "前端服务" cmd /c "cd frontend && npm run dev"

echo.
echo ========================================
echo  启动完成！
echo  后端: http://localhost:8000
echo  前端: http://localhost:3000
echo ========================================
pause
