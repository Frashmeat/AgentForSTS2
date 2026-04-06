[CmdletBinding()]
param(
    [Parameter(HelpMessage = "工作站后端端口。默认 7860。")]
    [Alias("wp")]
    [int]$WorkstationPort = 7860,

    [Parameter(HelpMessage = "本地静态前端端口。默认 8080。")]
    [Alias("fp")]
    [int]$FrontendPort = 8080,

    [Parameter(HelpMessage = "平台 Web API 基地址。默认 http://127.0.0.1:7870。")]
    [Alias("wb")]
    [string]$WebBaseUrl = "http://127.0.0.1:7870",

    [Parameter(HelpMessage = "启动后不自动打开浏览器。")]
    [Alias("nb")]
    [switch]$NoBrowser,

    [Parameter(HelpMessage = "仅打印将执行的动作，不真正启动进程。")]
    [Alias("Dry")]
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Resolve-LauncherLayout {
    param([string]$ScriptPath)

    $scriptDir = Split-Path -Path $ScriptPath -Parent
    $releaseRootCandidate = (Resolve-Path (Join-Path $scriptDir "..")).Path

    $repoRootCandidate = $null
    try {
        $repoRootCandidate = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
    } catch {
    }

    if ($null -ne $repoRootCandidate) {
        $repoBackend = Join-Path $repoRootCandidate "backend\main_workstation.py"
        if (Test-Path -LiteralPath $repoBackend) {
            return @{
                Mode = "repo"
                Root = $repoRootCandidate
                BackendRoot = Join-Path $repoRootCandidate "backend"
                FrontendRoot = Join-Path $repoRootCandidate "frontend"
                FrontendDist = Join-Path $repoRootCandidate "frontend\dist"
                ConfigPath = Join-Path $repoRootCandidate "config.json"
                ConfigExamplePath = Join-Path $repoRootCandidate "config.example.json"
                RuntimeDir = Join-Path $repoRootCandidate "runtime"
            }
        }
    }

    $releaseBackend = Join-Path $releaseRootCandidate "services\workstation\backend\main_workstation.py"
    if (Test-Path -LiteralPath $releaseBackend) {
        $serviceRoot = Join-Path $releaseRootCandidate "services\workstation"
        return @{
            Mode = "release"
            Root = $releaseRootCandidate
            BackendRoot = Join-Path $serviceRoot "backend"
            FrontendRoot = Join-Path $serviceRoot "frontend"
            FrontendDist = Join-Path $serviceRoot "frontend\dist"
            ConfigPath = Join-Path $serviceRoot "config.json"
            ConfigExamplePath = Join-Path $serviceRoot "config.example.json"
            RuntimeDir = Join-Path $releaseRootCandidate "runtime"
        }
    }

    throw "无法识别启动目录结构：既不是仓库工作区，也不是 workstation release 目录。"
}

function Ensure-ConfigFile {
    param(
        [string]$ConfigPath,
        [string]$ConfigExamplePath
    )

    if (Test-Path -LiteralPath $ConfigPath) {
        return
    }

    if (-not (Test-Path -LiteralPath $ConfigExamplePath)) {
        throw "缺少配置文件与配置模板：$ConfigPath"
    }

    Copy-Item -LiteralPath $ConfigExamplePath -Destination $ConfigPath -Force
}

function Resolve-PythonCommand {
    param([string]$BackendRoot)

    $venvPython = Join-Path $BackendRoot ".venv\Scripts\python.exe"
    if (Test-Path -LiteralPath $venvPython) {
        return $venvPython
    }

    $globalPython = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $globalPython) {
        return $globalPython.Source
    }

    throw "未找到可用的 Python 解释器。"
}

function Ensure-FrontendDist {
    param(
        [hashtable]$Layout,
        [switch]$DryRunMode
    )

    if (Test-Path -LiteralPath $Layout.FrontendDist) {
        return
    }

    if ($Layout.Mode -ne "repo") {
        throw "缺少前端构建产物：$($Layout.FrontendDist)"
    }

    $nodeModules = Join-Path $Layout.FrontendRoot "node_modules"
    if (-not (Test-Path -LiteralPath $nodeModules)) {
        throw "缺少前端依赖目录：$nodeModules"
    }

    if ($DryRunMode) {
        return
    }

    Push-Location $Layout.FrontendRoot
    try {
        & npm run build
        if ($LASTEXITCODE -ne 0) {
            throw "前端构建失败"
        }
    }
    finally {
        Pop-Location
    }
}

function Stop-ProcessListeningOnPort {
    param([int]$Port)

    $pids = @(
        netstat -ano 2>$null |
            Select-String ":$Port\s+.*LISTENING" |
            ForEach-Object { ($_ -split "\s+")[-1] } |
            Where-Object { $_ -match "^\d+$" } |
            Select-Object -Unique
    )

    foreach ($pid in $pids) {
        try {
            Stop-Process -Id ([int]$pid) -Force -ErrorAction Stop
        } catch {
        }
    }
}

function Write-RuntimeConfig {
    param(
        [string]$FrontendDist,
        [int]$ResolvedWorkstationPort,
        [string]$ResolvedWebBaseUrl
    )

    $runtimeConfigPath = Join-Path $FrontendDist "runtime-config.js"
    $content = @"
window.__AGENT_THE_SPIRE_API_BASES__ = {
  workstation: "http://127.0.0.1:$ResolvedWorkstationPort",
  web: "$ResolvedWebBaseUrl"
};

window.__AGENT_THE_SPIRE_WS_BASES__ = {
  workstation: "ws://127.0.0.1:$ResolvedWorkstationPort"
};
"@
    Set-Content -LiteralPath $runtimeConfigPath -Value $content -Encoding UTF8
    return $runtimeConfigPath
}

function Wait-WorkstationReady {
    param([int]$Port)

    for ($attempt = 0; $attempt -lt 30; $attempt += 1) {
        try {
            Invoke-WebRequest -Uri "http://127.0.0.1:$Port/api/config" -UseBasicParsing -TimeoutSec 2 | Out-Null
            return
        } catch {
            Start-Sleep -Seconds 1
        }
    }

    throw "等待 workstation-backend 就绪超时。"
}

$layout = Resolve-LauncherLayout -ScriptPath $PSCommandPath
$null = New-Item -ItemType Directory -Path $layout.RuntimeDir -Force
$statePath = Join-Path $layout.RuntimeDir "split-local-state.json"

Ensure-ConfigFile -ConfigPath $layout.ConfigPath -ConfigExamplePath $layout.ConfigExamplePath
Ensure-FrontendDist -Layout $layout -DryRunMode:$DryRun

$pythonCommand = Resolve-PythonCommand -BackendRoot $layout.BackendRoot
$runtimeConfigPath = Join-Path $layout.FrontendDist "runtime-config.js"

if ($DryRun) {
    Write-Host "DryRun 模式，不启动进程。"
    Write-Host "  模式            : $($layout.Mode)"
    Write-Host "  BackendRoot     : $($layout.BackendRoot)"
    Write-Host "  FrontendDist    : $($layout.FrontendDist)"
    Write-Host "  ConfigPath      : $($layout.ConfigPath)"
    Write-Host "  Python          : $pythonCommand"
    Write-Host "  RuntimeConfig   : $runtimeConfigPath"
    Write-Host "  Workstation URL : http://127.0.0.1:$WorkstationPort"
    Write-Host "  Frontend URL    : http://127.0.0.1:$FrontendPort"
    Write-Host "  Web API URL     : $WebBaseUrl"
    return
}

Stop-ProcessListeningOnPort -Port $WorkstationPort
Stop-ProcessListeningOnPort -Port $FrontendPort
$writtenRuntimeConfigPath = Write-RuntimeConfig -FrontendDist $layout.FrontendDist -ResolvedWorkstationPort $WorkstationPort -ResolvedWebBaseUrl $WebBaseUrl

$backendProcess = Start-Process -FilePath $pythonCommand -ArgumentList "main_workstation.py" -WorkingDirectory $layout.BackendRoot -PassThru

try {
    Wait-WorkstationReady -Port $WorkstationPort
} catch {
    try {
        Stop-Process -Id $backendProcess.Id -Force -ErrorAction Stop
    } catch {
    }
    throw
}

$frontendProcess = Start-Process -FilePath $pythonCommand -ArgumentList "-m", "http.server", "$FrontendPort", "--bind", "127.0.0.1", "--directory", $layout.FrontendDist -WorkingDirectory $layout.FrontendDist -PassThru

$state = @{
    workstation_pid = $backendProcess.Id
    frontend_pid = $frontendProcess.Id
    workstation_port = $WorkstationPort
    frontend_port = $FrontendPort
    web_base_url = $WebBaseUrl
    runtime_config = $writtenRuntimeConfigPath
    mode = $layout.Mode
}
$state | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $statePath -Encoding UTF8

$frontendUrl = "http://127.0.0.1:$FrontendPort"
Write-Host "独立前端 + workstation 已启动：$frontendUrl"
if (-not $NoBrowser) {
    Start-Process $frontendUrl | Out-Null
}
