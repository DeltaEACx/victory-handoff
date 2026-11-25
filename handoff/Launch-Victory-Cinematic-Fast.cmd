@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Run-Victory.ps1" -CinematicFast %*
set "code=%errorlevel%"
echo Victory exit code: %code%
pause
exit /b %code%
