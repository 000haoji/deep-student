@echo off
chcp 65001 >nul
title 错题管理系统 2.0
echo.
echo ========================================
echo  错题管理系统 2.0 - 启动中...
echo ========================================
echo.

:: 使用Python启动系统
python start_system_windows.py

:: 如果Python脚本执行失败，提示用户
if errorlevel 1 (
    echo.
    echo ========================================
    echo  启动失败！请检查：
    echo  1. 是否安装了 Python 3.8+
    echo  2. 是否安装了 Node.js 14+
    echo  3. 是否在项目根目录运行
    echo ========================================
    echo.
)

pause 