[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

function Resolve-LauncherRuntimeDir {
    param([string]$ScriptPath)

    $scriptDir = Split-Path -Path $ScriptPath -Parent
    $releaseRootCandidate = (Resolve-Path (Join-Path $scriptDir "..")).Path

    if (Test-Path -LiteralPath (Join-Path $releaseRootCandidate "runtime")) {
        return Join-Path $releaseRootCandidate "runtime"
    }

    try {
        $repoRootCandidate = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
        if (Test-Path -LiteralPath (Join-Path $repoRootCandidate "runtime")) {
            return Join-Path $repoRootCandidate "runtime"
        }
    } catch {
    }

    throw "未找到 runtime 目录。"
}

$runtimeDir = Resolve-LauncherRuntimeDir -ScriptPath $PSCommandPath
$statePath = Join-Path $runtimeDir "split-local-state.json"

if (-not (Test-Path -LiteralPath $statePath)) {
    Write-Host "未找到本地 launcher 状态文件，无需停止。"
    return
}

$state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json

foreach ($pid in @($state.frontend_pid, $state.workstation_pid)) {
    if ($null -eq $pid) {
        continue
    }

    try {
        Stop-Process -Id ([int]$pid) -Force -ErrorAction Stop
    } catch {
    }
}

Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
Write-Host "已停止本地独立前端与 workstation 进程。"
