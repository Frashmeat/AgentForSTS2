@echo off
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%start\start_web.bat" %*
exit /b %ERRORLEVEL%
