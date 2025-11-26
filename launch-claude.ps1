# PowerShell script to launch Claude Code with the proxy configured
# Usage: .\launch-claude.ps1

Write-Host "Setting HTTPS_PROXY to http://127.0.0.1:8080" -ForegroundColor Cyan
$env:HTTPS_PROXY = "http://127.0.0.1:8080"

Write-Host "Launching Claude Code..." -ForegroundColor Green
Write-Host "Make sure Anthropic Spy is already running in another terminal!" -ForegroundColor Yellow
Write-Host ""

claude-code
