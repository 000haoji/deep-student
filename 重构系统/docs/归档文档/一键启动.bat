@echo off
start "后端" cmd /k "cd /d H:\Deep-Students\重构系统\backend && venv\Scripts\activate && python -m uvicorn main:app --reload --host 127.0.0.1 --port 8000"
timeout /t 5 /nobreak > nul
start "前端" cmd /k "cd /d H:\Deep-Students\重构系统\frontend && npm run dev"
echo 启动完成！ 