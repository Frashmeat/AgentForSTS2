param(
    [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'

function Test-RequiredPath {
    param([string]$RelativePath)
    return Test-Path -LiteralPath (Join-Path $Root $RelativePath)
}

function Invoke-NpmInstall {
    param([string]$Reason)

    Write-Host "==> $Reason" -ForegroundColor Cyan
    Push-Location $Root
    try {
        npm install
        if ($LASTEXITCODE -ne 0) {
            throw "npm install failed"
        }
    } finally {
        Pop-Location
    }
}

$isWindowsHost = $PSVersionTable.PSEdition -eq 'Desktop' -or $IsWindows
$missing = @()

if (-not (Test-RequiredPath 'node_modules')) {
    $missing += 'node_modules'
}

if ($isWindowsHost) {
    if (-not (Test-RequiredPath 'node_modules\.bin\vite.cmd')) {
        $missing += 'node_modules\.bin\vite.cmd'
    }

    if (-not (Test-RequiredPath 'node_modules\@rollup\rollup-win32-x64-msvc')) {
        $missing += 'node_modules\@rollup\rollup-win32-x64-msvc'
    }
} else {
    if (-not (Test-RequiredPath 'node_modules/.bin/vite')) {
        $missing += 'node_modules/.bin/vite'
    }
}

if ($missing.Count -eq 0) {
    return
}

Invoke-NpmInstall "Installing or repairing npm dependencies (missing: $($missing -join ', '))"

$stillMissing = @()
foreach ($path in $missing) {
    if (-not (Test-RequiredPath $path)) {
        $stillMissing += $path
    }
}

if ($stillMissing.Count -gt 0) {
    $message = @"
npm dependencies are still incomplete: $($stillMissing -join ', ')

The local node_modules directory may have been created on another platform or left in a partial state.
Run these commands from the repository root, then retry:
  Remove-Item -Recurse -Force .\node_modules
  npm install
"@
    throw ($message.Trim())
}
