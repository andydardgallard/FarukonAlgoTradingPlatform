@echo off
chcp 65001 >nul
setlocal
rem Code Factory launcher for Windows — created by prepare_factory.
rem   start.cmd          — interactive (in the chat: /skill:code-factory)
rem   start.cmd --auto   — fully autonomous
cd /d "%~dp0"
rem Subagent model split (primary/secondary) is enabled here automatically.
set KIMI_CODE_EXPERIMENTAL_SECONDARY_MODEL=1
where kimi >nul 2>&1
if errorlevel 1 (
    echo The kimi command was not found. Install Kimi Code CLI and run again.
    pause
    exit /b 1
)
kimi %*
set "EXITCODE=%ERRORLEVEL%"
if not "%EXITCODE%"=="0" (
    echo The factory exited with code %EXITCODE%.
    pause
)
exit /b %EXITCODE%
