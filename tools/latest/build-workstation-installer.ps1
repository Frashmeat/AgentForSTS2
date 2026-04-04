[CmdletBinding()]
param(
    [string]$PythonVersion = "3.11.9",
    [switch]$SkipReleaseBuild,
    [switch]$SkipInstallerExe,
    [int]$Port = 7860
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$builder = Join-Path $repoRoot "tools\windows_installer\build_workstation_installer.py"

if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    throw "未找到 python 命令，无法执行 Windows 安装器构建脚本。"
}

$arguments = @(
    $builder,
    "--python-version", $PythonVersion,
    "--port", $Port
)

if ($SkipReleaseBuild) {
    $arguments += "--skip-release-build"
}

if ($SkipInstallerExe) {
    $arguments += "--skip-installer-exe"
}

Push-Location $repoRoot
try {
    & python @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Windows 安装器构建失败，退出码: $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}
