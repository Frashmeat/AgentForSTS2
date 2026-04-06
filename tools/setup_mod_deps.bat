@echo off
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%install\setup_mod_deps.bat" %*
exit /b %ERRORLEVEL%
