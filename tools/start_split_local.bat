@echo off
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%split-local\start_split_local.bat" %*
exit /b %ERRORLEVEL%
