# AgentTheSpire -- Web server dev mode
# Builds the frontend, then runs ats-web which serves it via rust-embed.
#
# Frontend changes require re-running this script (rust-embed inlines dist/ at
# compile time; no HMR). Backend code changes: Ctrl+C and re-run (incremental).

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

& "$PSScriptRoot\scripts\ensure-node-deps.ps1" -Root $PSScriptRoot

Write-Host '==> npm run build:web' -ForegroundColor Cyan
npm run build:web
if ($LASTEXITCODE -ne 0) { throw "frontend build failed" }

Write-Host '==> cargo run -p ats-web' -ForegroundColor Cyan
cargo run -p ats-web @args
exit $LASTEXITCODE
