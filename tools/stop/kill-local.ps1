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
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$servicePorts = @{
    frontend = [System.Collections.Generic.HashSet[int]]::new()
    workstation = [System.Collections.Generic.HashSet[int]]::new()
    web = [System.Collections.Generic.HashSet[int]]::new()
}
$protectedDockerProcessNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($processName in @("com.docker.backend", "wslrelay", "vpnkit", "docker")) {
    $null = $protectedDockerProcessNames.Add($processName)
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

function Stop-CurrentSessionLogMirroring {
    param([string[]]$ServiceNames = @())

    $serviceNameSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($serviceName in @($ServiceNames)) {
        if (-not [string]::IsNullOrWhiteSpace($serviceName)) {
            $null = $serviceNameSet.Add($serviceName)
        }
    }

    $registryVar = Get-Variable -Name ATS_LOCAL_LOG_MIRRORS -Scope Global -ErrorAction SilentlyContinue
    if ($null -ne $registryVar -and $registryVar.Value -is [hashtable]) {
        $registry = $registryVar.Value
        $keys = @($registry.Keys)
        foreach ($serviceName in $keys) {
            if ($serviceNameSet.Count -gt 0 -and (-not $serviceNameSet.Contains([string]$serviceName))) {
                continue
            }

            $entry = $registry[$serviceName]
            foreach ($sourceIdentifier in @($entry.SourceIdentifiers)) {
                Unregister-Event -SourceIdentifier $sourceIdentifier -ErrorAction SilentlyContinue
            }
            foreach ($job in @($entry.Jobs)) {
                if ($null -ne $job) {
                    Remove-Job -Id $job.Id -Force -ErrorAction SilentlyContinue
                }
            }
            foreach ($writer in @($entry.Writers)) {
                if ($null -ne $writer) {
                    try {
                        $writer.Dispose()
                    } catch {
                    }
                }
            }
            $registry.Remove($serviceName)
            Write-Host "[$serviceName] 已清理当前会话中的日志镜像句柄"
        }
    }

    $subscribers = Get-EventSubscriber -ErrorAction SilentlyContinue | Where-Object {
        $_.SourceIdentifier -like "ATS.LocalLogMirror.*"
    }
    foreach ($subscriber in @($subscribers)) {
        $parts = [string]$subscriber.SourceIdentifier -split "\."
        $serviceName = if ($parts.Length -ge 4) { $parts[2] } else { "" }
        if ($serviceNameSet.Count -gt 0 -and (-not $serviceNameSet.Contains($serviceName))) {
            continue
        }
        Unregister-Event -SubscriptionId $subscriber.SubscriptionId -ErrorAction SilentlyContinue
    }

    $jobs = Get-Job -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -like "ATS.LocalLogMirror.*"
    }
    foreach ($job in @($jobs)) {
        $parts = [string]$job.Name -split "\."
        $serviceName = if ($parts.Length -ge 4) { $parts[2] } else { "" }
        if ($serviceNameSet.Count -gt 0 -and (-not $serviceNameSet.Contains($serviceName))) {
            continue
        }
        Remove-Job -Id $job.Id -Force -ErrorAction SilentlyContinue
    }
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

function Get-ProcessDescriptor {
    param([int]$ProcessId)

    try {
        $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        if ($null -eq $process) {
            return $null
        }

        $cimProcess = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction SilentlyContinue
        return [pscustomobject]@{
            Process = $process
            ProcessName = [string]$process.ProcessName
            ExecutablePath = if ($null -ne $cimProcess) { [string]$cimProcess.ExecutablePath } else { "" }
            CommandLine = if ($null -ne $cimProcess) { [string]$cimProcess.CommandLine } else { "" }
        }
    } catch {
        return $null
    }
}

function Test-TextContains {
    param(
        [string]$Text,
        [string]$Fragment
    )

    if ([string]::IsNullOrWhiteSpace($Text) -or [string]::IsNullOrWhiteSpace($Fragment)) {
        return $false
    }

    return $Text.IndexOf($Fragment, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
}

function Test-TextContainsAny {
    param(
        [string]$Text,
        [string[]]$Fragments
    )

    foreach ($fragment in @($Fragments)) {
        if (Test-TextContains -Text $Text -Fragment $fragment) {
            return $true
        }
    }

    return $false
}

function Test-PathWithinRepo {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }

    try {
        $fullPath = [System.IO.Path]::GetFullPath($Path)
        return $fullPath.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)
    } catch {
        return $false
    }
}

function Test-IsProtectedDockerProcess {
    param([object]$Descriptor)

    if ($null -eq $Descriptor) {
        return $false
    }

    if ($protectedDockerProcessNames.Contains([string]$Descriptor.ProcessName)) {
        return $true
    }

    if (Test-TextContainsAny -Text $Descriptor.CommandLine -Fragments @("docker desktop", "com.docker.backend", "dockerbackendapiserver", "dockerdesktoplinuxengine")) {
        return $true
    }

    if (Test-TextContainsAny -Text $Descriptor.ExecutablePath -Fragments @("Docker\Docker", "Docker Desktop")) {
        return $true
    }

    return $false
}

function Test-IsExpectedServiceProcess {
    param(
        [string]$ServiceName,
        [object]$Descriptor
    )

    if ($null -eq $Descriptor) {
        return $false
    }

    $serviceKey = [string]$ServiceName
    $commandLine = [string]$Descriptor.CommandLine
    $executablePath = [string]$Descriptor.ExecutablePath
    $processName = [string]$Descriptor.ProcessName
    $isRepoOwned = (Test-PathWithinRepo -Path $executablePath) -or (Test-TextContains -Text $commandLine -Fragment $repoRoot)

    switch -Regex ($serviceKey) {
        "^frontend$" {
            if ($processName -match "^(?i:python|python3|py|node|npm)$" -and $isRepoOwned) {
                if (Test-TextContainsAny -Text $commandLine -Fragments @("http.server", "vite", "frontend")) {
                    return $true
                }
            }
            return $false
        }
        "^workstation$" {
            if ($processName -match "^(?i:python|python3|py)$" -and $isRepoOwned) {
                if (Test-TextContainsAny -Text $commandLine -Fragments @("main_workstation.py", "main_workstation:app", "uvicorn")) {
                    return $true
                }
            }
            return $false
        }
        "^web$" {
            if (Test-IsProtectedDockerProcess -Descriptor $Descriptor) {
                return $false
            }
            if ($processName -match "^(?i:python|python3|py)$" -and $isRepoOwned) {
                if (Test-TextContainsAny -Text $commandLine -Fragments @("main_web.py", "main_web:app", "uvicorn")) {
                    return $true
                }
            }
            return $false
        }
        "^log-viewer$" {
            return $processName -match "^(?i:powershell|pwsh|cmd|WindowsTerminal)$"
        }
        default {
            return $true
        }
    }
}

function Stop-ProcessById {
    param(
        [string]$Name,
        [int]$ProcessId,
        [string]$Detail = "",
        [switch]$SkipServiceValidation
    )

    if ($ProcessId -le 0) {
        return $false
    }

    if ($stoppedProcessIds.Contains($ProcessId)) {
        return $true
    }

    try {
        $descriptor = Get-ProcessDescriptor -ProcessId $ProcessId
        if ($null -eq $descriptor) {
            if ([string]::IsNullOrWhiteSpace($Detail)) {
                Write-Host "[$Name] PID $ProcessId 已不存在，无需停止"
            } else {
                Write-Host "[$Name] PID $ProcessId 已不存在（$Detail）"
            }
            return $false
        }

        if (-not $SkipServiceValidation) {
            if (Test-IsProtectedDockerProcess -Descriptor $descriptor) {
                Write-Host "[$Name] 跳过 PID $ProcessId ($($descriptor.ProcessName))，检测为 Docker/WSL 宿主代理进程"
                return $false
            }

            if (-not (Test-IsExpectedServiceProcess -ServiceName $Name -Descriptor $descriptor)) {
                Write-Warning "[$Name] 跳过 PID $ProcessId ($($descriptor.ProcessName))，未识别为当前仓库的本地服务进程"
                return $false
            }
        }

        Stop-Process -Id $ProcessId -Force -ErrorAction Stop
        $null = $stoppedProcessIds.Add($ProcessId)
        if ([string]::IsNullOrWhiteSpace($Detail)) {
            Write-Host "[$Name] 已停止 PID $ProcessId ($($descriptor.ProcessName))"
        } else {
            Write-Host "[$Name] 已停止 PID $ProcessId ($($descriptor.ProcessName))，$Detail"
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

        Stop-CurrentSessionLogMirroring -ServiceNames @($serviceName)

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
                $null = Stop-ProcessById -Name "log-viewer" -ProcessId ([int]$viewerPid) -Detail "日志窗口" -SkipServiceValidation
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

function Stop-DockerComposeRelease {
    param([string]$ReleaseRoot)

    $composeFile = Join-Path $ReleaseRoot "docker-compose.yml"
    $runtimeDir = Join-Path $ReleaseRoot "runtime"
    $envFile = Join-Path $runtimeDir ".env"
    $webServiceDir = Join-Path (Join-Path $ReleaseRoot "services") "web"

    if ((-not (Test-Path -LiteralPath $composeFile)) -or (-not (Test-Path -LiteralPath $webServiceDir))) {
        return $false
    }

    $projectName = Split-Path -Leaf $ReleaseRoot
    Write-Host "[docker-web] 停止 compose 项目 $projectName ($ReleaseRoot)"

    Push-Location $ReleaseRoot
    try {
        if (Test-Path -LiteralPath $envFile) {
            & docker compose --project-name $projectName --env-file $envFile -f $composeFile down --remove-orphans
        } else {
            & docker compose --project-name $projectName -f $composeFile down --remove-orphans
        }

        if ($LASTEXITCODE -ne 0) {
            Write-Warning "[docker-web] 停止 compose 项目失败，退出码: $LASTEXITCODE ($ReleaseRoot)"
            return $false
        }

        Write-Host "[docker-web] 已停止 compose 项目 $projectName"
        return $true
    } finally {
        Pop-Location
    }
}

function Stop-DockerComposeWebServices {
    $dockerCommand = Get-Command docker -ErrorAction SilentlyContinue
    if ($null -eq $dockerCommand) {
        Write-Host "[docker-web] 未找到 docker，跳过 Docker web 服务停止"
        return
    }

    $artifactsRoot = Join-Path $PSScriptRoot "latest\artifacts"
    if (-not (Test-Path -LiteralPath $artifactsRoot)) {
        return
    }

    $releaseRoots = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($directory in Get-ChildItem -LiteralPath $artifactsRoot -Directory -ErrorAction SilentlyContinue) {
        $composeFile = Join-Path $directory.FullName "docker-compose.yml"
        $webServiceDir = Join-Path (Join-Path $directory.FullName "services") "web"
        if ((Test-Path -LiteralPath $composeFile) -and (Test-Path -LiteralPath $webServiceDir)) {
            $null = $releaseRoots.Add($directory.FullName)
        }
    }

    foreach ($releaseRoot in $releaseRoots) {
        $null = Stop-DockerComposeRelease -ReleaseRoot $releaseRoot
    }
}

Discover-ServicePorts

Stop-CurrentSessionLogMirroring -ServiceNames @("frontend", "workstation", "web")

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
Stop-DockerComposeWebServices
