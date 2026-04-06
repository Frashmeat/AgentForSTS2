@echo off
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%split-local\stop_split_local.bat" %*
exit /b %ERRORLEVEL%
