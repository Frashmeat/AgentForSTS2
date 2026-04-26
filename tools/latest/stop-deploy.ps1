<#
.SYNOPSIS
停止 deploy-docker.ps1 启动的本地服务进程。

.DESCRIPTION
读取 release/runtime/local-deploy-state.json，停止 `deploy-docker.ps1`
以本机模式拉起的 `workstation` / `frontend` 进程，并关闭对应日志窗口。
默认只处理本地服务进程，不额外执行 Docker down。

.PARAMETER Target
部署目标。可选 hybrid / workstation / frontend / web。

.PARAMETER ReleaseRoot
release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。

.PARAMETER Help
显示帮助说明并退出。

.EXAMPLE
pwsh -File .\tools\latest\stop-deploy.ps1 hybrid

.EXAMPLE
pwsh -File .\tools\latest\stop-deploy.ps1 workstation -ReleaseRoot .\tools\latest\artifacts\agentthespire-workstation-release
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0, HelpMessage = "部署目标。可选 hybrid / workstation / frontend / web。")]
    [Alias("t")]
    [ValidateSet("hybrid", "workstation", "frontend", "web")]
    [string]$Target = "hybrid",

    [Parameter(HelpMessage = "release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。")]
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Parameter(HelpMessage = "显示帮助说明并退出。")]
    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help -or $PSBoundParameters.Count -eq 0) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

function Get-LocalDeploymentStatePath {
    param([string]$ResolvedReleaseRoot)

    return Join-Path (Join-Path $ResolvedReleaseRoot "runtime") "local-deploy-state.json"
}

function Get-LogViewerPidPath {
    param(
        [string]$ResolvedReleaseRoot,
        [string]$ServiceName
    )

    return Join-Path (Join-Path $ResolvedReleaseRoot "runtime") ("{0}.log-viewer.pid" -f $ServiceName)
}

function Stop-LogViewerWindow {
    param(
        [string]$ResolvedReleaseRoot,
        [string]$ServiceName
    )

    $pidPath = Get-LogViewerPidPath -ResolvedReleaseRoot $ResolvedReleaseRoot -ServiceName $ServiceName
    if (-not (Test-Path -LiteralPath $pidPath)) {
        return
    }

    $viewerProcessId = ""
    try {
        $viewerProcessId = (Get-Content -LiteralPath $pidPath -Raw).Trim()
    } catch {
        $viewerProcessId = ""
    }

    if ($viewerProcessId -match "^\d+$") {
        try {
            Stop-Process -Id ([int]$viewerProcessId) -Force -ErrorAction Stop
        } catch {
        }
    }

    Remove-Item -LiteralPath $pidPath -Force -ErrorAction SilentlyContinue
}

$effectiveReleaseRoot = if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    Join-Path $PSScriptRoot ("artifacts\agentthespire-{0}-release" -f $Target)
} else {
    $ReleaseRoot
}

$statePath = Get-LocalDeploymentStatePath -ResolvedReleaseRoot $effectiveReleaseRoot
if (-not (Test-Path -LiteralPath $statePath)) {
    Write-Host "未找到本地部署状态文件，无需停止：$statePath"
    return
}

$state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
$stoppedServices = @()

foreach ($entry in @($state.processes)) {
    if ($null -eq $entry) {
        continue
    }

    $serviceName = [string]$entry.service_name
    if (-not [string]::IsNullOrWhiteSpace($serviceName)) {
        Stop-LogViewerWindow -ResolvedReleaseRoot $effectiveReleaseRoot -ServiceName $serviceName
    }

    $recordedProcessId = $entry.pid
    if ($null -eq $recordedProcessId) {
        continue
    }

    try {
        Stop-Process -Id ([int]$recordedProcessId) -Force -ErrorAction Stop
    } catch {
    }

    if (-not [string]::IsNullOrWhiteSpace($serviceName)) {
        $stoppedServices += $serviceName
    }
}

Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue

if ($stoppedServices.Count -gt 0) {
    Write-Host ("已停止本地部署进程：{0}" -f (($stoppedServices | Select-Object -Unique) -join ", "))
} else {
    Write-Host "已清理本地部署状态文件，未记录可停止的本地服务。"
}
