# AgentTheSpire -- Tauri desktop dev mode
# Launches `npx tauri dev`: Vite HMR for frontend + cargo incremental for Rust.
#
# Usage:
#   .\dev.ps1                     # ml-rembg OFF (default)
#   .\dev.ps1 -MlRembg            # enable ML background removal for testing
#   .\dev.ps1 --verbose           # extra args pass through to tauri dev

[CmdletBinding()]
param(
    [switch]$MlRembg,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgsExtra
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

& "$PSScriptRoot\scripts\ensure-node-deps.ps1" -Root $PSScriptRoot

# To enable signing later, set these before `npx tauri dev`:
# $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\agentthespire.key" -Raw
# $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '...'

$tauriArgs = @()
if ($MlRembg) {
    Write-Host '==> ml-rembg feature ON (first run downloads onnxruntime native lib)' -ForegroundColor Yellow
    $tauriArgs += '--features'
    $tauriArgs += 'ml-rembg'
}

Write-Host '==> npx tauri dev' -ForegroundColor Cyan
npx tauri dev @tauriArgs @TauriArgsExtra
exit $LASTEXITCODE
