# AgentTheSpire -- Tauri desktop production build
# Outputs installers: MSI/NSIS (Windows), DMG (macOS), AppImage/DEB (Linux).
# Artifacts land under src-tauri/target/release/bundle/.
#
# Usage:
#   .\build.ps1                   # CI-equivalent build, ml-rembg OFF
#   .\build.ps1 -MlRembg          # enable ML background removal (adds ~200MB)
#   .\build.ps1 --verbose         # extra args pass through to tauri build

[CmdletBinding()]
param(
    [switch]$MlRembg
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if (-not (Test-Path 'node_modules')) {
    Write-Host '==> Installing npm dependencies (first run)' -ForegroundColor Cyan
    npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
}

# To enable signing later, set these before `npx tauri build`:
# $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\agentthespire.key" -Raw
# $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '...'

$tauriArgs = @()
if ($MlRembg) {
    Write-Host '==> ml-rembg feature ON (adds ~200MB ort + onnxruntime native lib)' -ForegroundColor Yellow
    $tauriArgs += '--features'
    $tauriArgs += 'ml-rembg'
}

Write-Host '==> npx tauri build' -ForegroundColor Cyan
npx tauri build @tauriArgs @args
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$bundleDir = Join-Path $PSScriptRoot 'src-tauri\target\release\bundle'
if (Test-Path $bundleDir) {
    Write-Host "`n==> Bundle artifacts" -ForegroundColor Green
    Get-ChildItem -Recurse -File $bundleDir |
        Where-Object { $_.Extension -in '.msi', '.exe', '.dmg', '.deb', '.AppImage', '.app' } |
        ForEach-Object { Write-Host "  $($_.FullName)" }
}
