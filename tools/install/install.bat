@echo off
setlocal
set "SCRIPT_DIR=%~dp0"

powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%install.ps1" %*
set "RC=%ERRORLEVEL%"

echo.
if "%RC%"=="0" (
    echo [INFO] install.ps1 completed
) else (
    echo [ERROR] install.ps1 failed, exit code: %RC%
)
pause
exit /b %RC%
