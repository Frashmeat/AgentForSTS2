@echo off
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%install\install.bat" %*
exit /b %ERRORLEVEL%
