@echo off
REM Batch script to launch Claude Code with the proxy configured
REM Usage: launch-claude.bat

echo Setting HTTPS_PROXY to http://127.0.0.1:8080
set HTTPS_PROXY=http://127.0.0.1:8080

echo.
echo Launching Claude Code...
echo Make sure Anthropic Spy is already running in another terminal!
echo.

claude-code
