@echo off
echo 重启后端服务...

:: 切换到后端目录
cd backend

:: 查找并终止已有的uvicorn进程
echo 停止旧的后端服务...
taskkill /F /IM python.exe /FI "WINDOWTITLE eq uvicorn*" 2>nul
taskkill /F /IM uvicorn.exe 2>nul

:: 等待2秒确保进程已终止
timeout /t 2 /nobreak

:: 重新启动后端服务
echo 启动新的后端服务...
start cmd /c "python -m uvicorn main:app --reload --host 0.0.0.0 --port 8000"

echo 后端服务正在重启...
echo 请等待几秒钟后刷新浏览器
timeout /t 5

echo 重启完成！
pause 