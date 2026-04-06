<#
.SYNOPSIS
构建 Windows 工作站安装器。

.DESCRIPTION
调用 windows installer 构建脚本，按需打包 release 并生成安装器 EXE。
直接执行脚本且不传任何参数时，会默认显示本帮助而不是立即开始构建。

.PARAMETER PythonVersion
Python 版本。默认 3.11.9。

.PARAMETER SkipReleaseBuild
跳过 release 打包，直接复用现有 release 目录。

.PARAMETER SkipInstallerExe
跳过安装器 EXE 生成，仅准备中间产物。

.PARAMETER Port
工作站端口。默认 7860。

.PARAMETER Help
显示帮助说明并退出。

.EXAMPLE
pwsh -File .\tools\latest\build-workstation-installer.ps1

.EXAMPLE
pwsh -File .\tools\latest\build-workstation-installer.ps1 -py 3.11.9 -p 7860 -NoExe
#>
[CmdletBinding()]
param(
    # 基础参数
    [Parameter(HelpMessage = "Python 版本。默认 3.11.9。")]
    [Alias("py")]
    [string]$PythonVersion = "3.11.9",

    # 行为开关
    [Parameter(HelpMessage = "跳过 release 打包，直接复用现有 release 目录。")]
    [Alias("NoRelease")]
    [switch]$SkipReleaseBuild,

    [Parameter(HelpMessage = "跳过安装器 EXE 生成，仅准备中间产物。")]
    [Alias("NoExe")]
    [switch]$SkipInstallerExe,

    # 端口参数
    [Parameter(HelpMessage = "工作站端口。默认 7860。")]
    [Alias("p")]
    [int]$Port = 7860,

    [Parameter(HelpMessage = "显示帮助说明并退出。")]
    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help -or $PSBoundParameters.Count -eq 0) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

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
