@echo off
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%start\start_workstation.bat" %*
exit /b %ERRORLEVEL%
