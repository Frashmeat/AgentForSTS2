@echo off
chcp 65001 >nul
title AgentTheSpire

:: 清理占用 7860 端口的旧进程
for /f "tokens=5" %%a in ('netstat -ano 2^>nul ^| findstr ":7860 " ^| findstr "LISTENING"') do (
    echo 清理旧进程 PID %%a...
    taskkill /PID %%a /F >nul 2>&1
)
timeout /t 1 >nul

cd /d "%~dp0backend"
echo 启动 AgentTheSpire...
echo 打开浏览器访问 http://localhost:7860
start /b cmd /c "timeout /t 3 >nul && start "" http://localhost:7860"
.venv\Scripts\python.exe main.py
