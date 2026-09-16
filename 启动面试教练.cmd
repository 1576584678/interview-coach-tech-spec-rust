@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo Starting Interview Coach (debug build)...
echo   http://127.0.0.1:8080
echo.
bin\interview-coach.exe %*
if errorlevel 1 pause
