<#
.SYNOPSIS
Run the backend pytest suite with the project Python environment.

.DESCRIPTION
Uses backend/.venv as the single project Python environment, so pytest
does not depend on the active shell PATH or the system Python installation.
#>
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$PytestArgs = @()
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$toolsRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $toolsRoot ".."))
$backendPython = Join-Path $repoRoot "backend\.venv\Scripts\python.exe"

if (-not (Test-Path -LiteralPath $backendPython)) {
    throw "backend/.venv Python was not found: $backendPython. Run: powershell -File .\tools\tools.ps1 install"
}

Push-Location $repoRoot
try {
    & $backendPython -m pytest @PytestArgs
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
