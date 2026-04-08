[CmdletBinding()]
param(
    [Parameter(HelpMessage = "前端端口。显式传入时优先使用。")]
    [int]$FrontendPort = 0,

    [Parameter(HelpMessage = "工作站后端端口。显式传入时优先使用。")]
    [int]$WorkstationPort = 0,

    [Parameter(HelpMessage = "Web 后端端口。显式传入时优先使用。")]
    [int]$WebPort = 0
)

$ErrorActionPreference = "Stop"
$stoppedProcessIds = [System.Collections.Generic.HashSet[int]]::new()
$servicePorts = @{
    frontend = [System.Collections.Generic.HashSet[int]]::new()
    workstation = [System.Collections.Generic.HashSet[int]]::new()
    web = [System.Collections.Generic.HashSet[int]]::new()
}

function Add-ServicePort {
    param(
        [string]$ServiceName,
        [int]$Port
    )

    if ($Port -le 0) {
        return
    }

    if (-not $servicePorts.ContainsKey($ServiceName)) {
        $servicePorts[$ServiceName] = [System.Collections.Generic.HashSet[int]]::new()
    }

    $null = $servicePorts[$ServiceName].Add($Port)
}

function Get-ServicePorts {
    param([string]$ServiceName)

    if (-not $servicePorts.ContainsKey($ServiceName)) {
        return @()
    }

    return @($servicePorts[$ServiceName])
}

function Get-PidsByPort {
    param([int]$Port)

    $resolvedPids = @()

    if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
        $resolvedPids = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
            Select-Object -ExpandProperty OwningProcess -Unique
    }

    if (-not $resolvedPids -or $resolvedPids.Count -eq 0) {
        $netstatLines = netstat -ano | Select-String ":$Port\s+.*LISTENING"
        foreach ($line in $netstatLines) {
            $parts = ($line -split "\s+") | Where-Object { $_ -ne "" }
            if ($parts.Count -ge 5) {
                $resolvedPids += [int]$parts[-1]
            }
        }
        $resolvedPids = $resolvedPids | Select-Object -Unique
    }

    return @($resolvedPids)
}

function Stop-ProcessById {
    param(
        [string]$Name,
        [int]$ProcessId,
        [string]$Detail = ""
    )

    if ($ProcessId -le 0) {
        return $false
    }

    if ($stoppedProcessIds.Contains($ProcessId)) {
        return $true
    }

    try {
        $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        if ($null -eq $process) {
            if ([string]::IsNullOrWhiteSpace($Detail)) {
                Write-Host "[$Name] PID $ProcessId 已不存在，无需停止"
            } else {
                Write-Host "[$Name] PID $ProcessId 已不存在（$Detail）"
            }
            return $false
        }
        Stop-Process -Id $ProcessId -Force -ErrorAction Stop
        $null = $stoppedProcessIds.Add($ProcessId)
        if ([string]::IsNullOrWhiteSpace($Detail)) {
            Write-Host "[$Name] 已停止 PID $ProcessId ($($process.ProcessName))"
        } else {
            Write-Host "[$Name] 已停止 PID $ProcessId ($($process.ProcessName))，$Detail"
        }
        return $true
    } catch {
        Write-Warning "[$Name] 停止 PID $ProcessId 失败: $($_.Exception.Message)"
        return $false
    }
}

function Stop-ServiceByPort {
    param(
        [string]$Name,
        [int]$Port
    )

    $pids = Get-PidsByPort -Port $Port

    if (-not $pids -or $pids.Count -eq 0) {
        Write-Host "[$Name] 端口 $Port 未发现监听进程"
        return
    }

    foreach ($processId in $pids) {
        $null = Stop-ProcessById -Name $Name -ProcessId $processId -Detail "端口 $Port"
    }
}

function Read-JsonFile {
    param([string]$Path)

    try {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    } catch {
        return $null
    }
}

function Register-PortsFromRuntimeConfig {
    param([string]$ConfigPath)

    if (-not (Test-Path -LiteralPath $ConfigPath)) {
        return
    }

    $config = Read-JsonFile -Path $ConfigPath
    if ($null -eq $config) {
        return
    }

    if ($config.runtime -and $config.runtime.workstation -and $config.runtime.workstation.port) {
        Add-ServicePort -ServiceName "workstation" -Port ([int]$config.runtime.workstation.port)
    }

    if ($config.runtime -and $config.runtime.web -and $config.runtime.web.port) {
        Add-ServicePort -ServiceName "web" -Port ([int]$config.runtime.web.port)
    }
}

function Register-PortsFromSplitLocalState {
    param([string]$StatePath)

    if (-not (Test-Path -LiteralPath $StatePath)) {
        return
    }

    $state = Read-JsonFile -Path $StatePath
    if ($null -eq $state) {
        return
    }

    if ($state.frontend_port) {
        Add-ServicePort -ServiceName "frontend" -Port ([int]$state.frontend_port)
    }
    if ($state.workstation_port) {
        Add-ServicePort -ServiceName "workstation" -Port ([int]$state.workstation_port)
    }
}

function Discover-ServicePorts {
    if ($FrontendPort -gt 0) {
        Add-ServicePort -ServiceName "frontend" -Port $FrontendPort
    }
    if ($WorkstationPort -gt 0) {
        Add-ServicePort -ServiceName "workstation" -Port $WorkstationPort
    }
    if ($WebPort -gt 0) {
        Add-ServicePort -ServiceName "web" -Port $WebPort
    }

    $runtimeConfigPaths = @(
        (Join-Path $PSScriptRoot "..\runtime\workstation.config.json"),
        (Join-Path $PSScriptRoot "latest\artifacts\agentthespire-hybrid-release\runtime\workstation.config.json"),
        (Join-Path $PSScriptRoot "latest\artifacts\agentthespire-workstation-release\runtime\workstation.config.json"),
        (Join-Path $PSScriptRoot "latest\artifacts\agentthespire-full-release\runtime\workstation.config.json"),
        (Join-Path $PSScriptRoot "latest\artifacts\agentthespire-web-release\runtime\workstation.config.json")
    ) | Select-Object -Unique

    foreach ($runtimeConfigPath in $runtimeConfigPaths) {
        Register-PortsFromRuntimeConfig -ConfigPath $runtimeConfigPath
    }

    $artifactsRoot = Join-Path $PSScriptRoot "latest\artifacts"
    if (Test-Path -LiteralPath $artifactsRoot) {
        foreach ($runtimeConfigPath in Get-ChildItem -LiteralPath $artifactsRoot -Recurse -Filter "workstation.config.json" -ErrorAction SilentlyContinue) {
            Register-PortsFromRuntimeConfig -ConfigPath $runtimeConfigPath.FullName
        }
        foreach ($splitStatePath in Get-ChildItem -LiteralPath $artifactsRoot -Recurse -Filter "split-local-state.json" -ErrorAction SilentlyContinue) {
            Register-PortsFromSplitLocalState -StatePath $splitStatePath.FullName
        }
    }

    Register-PortsFromSplitLocalState -StatePath (Join-Path $PSScriptRoot "..\runtime\split-local-state.json")
}

function Stop-ProcessesFromLocalDeployState {
    param([string]$StatePath)

    if (-not (Test-Path -LiteralPath $StatePath)) {
        return
    }

    try {
        $state = Get-Content -LiteralPath $StatePath -Raw | ConvertFrom-Json
    } catch {
        Write-Warning "[local-deploy] 读取状态文件失败: $StatePath"
        return
    }

    $releaseRoot = Split-Path -Parent (Split-Path -Parent $StatePath)

    foreach ($entry in @($state.processes)) {
        if ($null -eq $entry) {
            continue
        }

        $serviceName = [string]$entry.service_name
        if ([string]::IsNullOrWhiteSpace($serviceName)) {
            $serviceName = "local-deploy"
        }

        $detail = if ($entry.port) {
            "来自状态文件 $(Split-Path -Leaf $StatePath)，端口 $($entry.port)"
        } else {
            "来自状态文件 $(Split-Path -Leaf $StatePath)"
        }

        $stopped = $false
        if ($entry.pid) {
            $stopped = Stop-ProcessById -Name $serviceName -ProcessId ([int]$entry.pid) -Detail $detail
        }

        if (-not $stopped -and $entry.port) {
            Stop-ServiceByPort -Name $serviceName -Port ([int]$entry.port)
        }
    }

    Remove-Item -LiteralPath $StatePath -Force -ErrorAction SilentlyContinue

    foreach ($viewerPidPath in Get-ChildItem -LiteralPath (Join-Path $releaseRoot "runtime") -Filter "*.log-viewer.pid" -ErrorAction SilentlyContinue) {
        try {
            $viewerPid = (Get-Content -LiteralPath $viewerPidPath.FullName -Raw).Trim()
            if ($viewerPid -match "^\d+$") {
                $null = Stop-ProcessById -Name "log-viewer" -ProcessId ([int]$viewerPid) -Detail "日志窗口"
            }
        } catch {
        }
        Remove-Item -LiteralPath $viewerPidPath.FullName -Force -ErrorAction SilentlyContinue
    }
}

function Stop-ProcessesFromArtifacts {
    $artifactsRoot = Join-Path $PSScriptRoot "latest\artifacts"
    if (-not (Test-Path -LiteralPath $artifactsRoot)) {
        return
    }

    $stateFiles = Get-ChildItem -LiteralPath $artifactsRoot -Recurse -Filter "local-deploy-state.json" -ErrorAction SilentlyContinue
    foreach ($stateFile in $stateFiles) {
        Stop-ProcessesFromLocalDeployState -StatePath $stateFile.FullName
    }
}

Discover-ServicePorts

foreach ($resolvedFrontendPort in Get-ServicePorts -ServiceName "frontend") {
    Stop-ServiceByPort -Name "frontend" -Port $resolvedFrontendPort
}

foreach ($resolvedWorkstationPort in Get-ServicePorts -ServiceName "workstation") {
    Stop-ServiceByPort -Name "workstation" -Port $resolvedWorkstationPort
}

foreach ($resolvedWebPort in Get-ServicePorts -ServiceName "web") {
    Stop-ServiceByPort -Name "web" -Port $resolvedWebPort
}

Stop-ProcessesFromArtifacts
