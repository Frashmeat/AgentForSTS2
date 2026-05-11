# AgentTheSpire -- Tauri desktop dev mode
# Launches `npx tauri dev`: Vite HMR for frontend + cargo incremental for Rust.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if (-not (Test-Path 'node_modules')) {
    Write-Host '==> Installing npm dependencies (first run)' -ForegroundColor Cyan
    npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
}

# To enable signing later, set these before `npx tauri dev`:
# $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\agentthespire.key" -Raw
# $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '...'

Write-Host '==> npx tauri dev' -ForegroundColor Cyan
npx tauri dev @args
exit $LASTEXITCODE
