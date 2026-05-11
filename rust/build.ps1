# AgentTheSpire -- Tauri desktop production build
# Outputs installers: MSI/NSIS (Windows), DMG (macOS), AppImage/DEB (Linux).
# Artifacts land under src-tauri/target/release/bundle/.

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

Write-Host '==> npx tauri build' -ForegroundColor Cyan
npx tauri build @args
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$bundleDir = Join-Path $PSScriptRoot 'src-tauri\target\release\bundle'
if (Test-Path $bundleDir) {
    Write-Host "`n==> Bundle artifacts" -ForegroundColor Green
    Get-ChildItem -Recurse -File $bundleDir |
        Where-Object { $_.Extension -in '.msi', '.exe', '.dmg', '.deb', '.AppImage', '.app' } |
        ForEach-Object { Write-Host "  $($_.FullName)" }
}
