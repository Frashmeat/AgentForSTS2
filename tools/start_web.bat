@echo off
chcp 65001 >nul
title AgentTheSpire Web Backend

set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "ROOT_DIR=%%~fI"

for /f "tokens=5" %%a in ('netstat -ano 2^>nul ^| findstr ":7870 " ^| findstr "LISTENING"') do (
    echo 清理旧 Web 后端进程 PID %%a...
    taskkill /PID %%a /F >nul 2>&1
)
timeout /t 1 >nul

cd /d "%ROOT_DIR%\backend"
echo 启动 web-backend...
echo 监听地址 http://localhost:7870
echo 注意：web-backend 需要有效的 database.url 配置。
.venv\Scripts\python.exe main_web.py
