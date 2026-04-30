<#
.SYNOPSIS
AgentTheSpire 桌面端工作站 Docker 启停脚手架。

.DESCRIPTION
包装 docker compose 命令，固定使用 docker-compose.workstation.yml + .env.workstation。
首次使用先复制 .env.workstation.example -> .env.workstation 并按需编辑。

.PARAMETER Action
动作。可选: up / down / restart / build / rebuild / logs / ps / shell / config

.PARAMETER Detach
up / restart 时使用 -d 后台运行。默认开启。

.PARAMETER Follow
logs 时使用 -f 持续跟随。

.EXAMPLE
pwsh -File .\tools\docker\workstation.ps1 up

.EXAMPLE
pwsh -File .\tools\docker\workstation.ps1 logs -Follow

.EXAMPLE
pwsh -File .\tools\docker\workstation.ps1 rebuild

.EXAMPLE
pwsh -File .\tools\docker\workstation.ps1 test
pwsh -File .\tools\docker\workstation.ps1 test tests/platform/services/test_execution_orchestrator_service.py -- -x -k start_execution

.EXAMPLE
pwsh -File .\tools\docker\workstation.ps1 exec -- python --version

.EXAMPLE
pwsh -File .\tools\docker\workstation.ps1 lint        # ruff check + black --check
pwsh -File .\tools\docker\workstation.ps1 lint -Fix   # ruff 自动修 + black 重排
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet("up", "down", "restart", "build", "rebuild", "logs", "ps", "shell", "config", "test", "exec", "lint")]
    [string]$Action = "up",

    [switch]$Detach = $true,
    [switch]$Follow,
    [switch]$Fix,

    # 透传给 test / exec 的剩余参数（pytest 路径与开关、自定义命令等）
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Rest
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$composeFile = Join-Path $repoRoot "docker-compose.workstation.yml"
$envFile     = Join-Path $repoRoot ".env.workstation"
$envExample  = Join-Path $repoRoot ".env.workstation.example"
$configFile  = Join-Path $repoRoot "runtime\workstation.config.json"
$configExample = Join-Path $repoRoot "config.example.json"

if (-not (Test-Path $composeFile)) {
    throw "未找到 $composeFile"
}

if (-not (Test-Path $envFile)) {
    if (Test-Path $envExample) {
        Write-Host "[init] 复制 .env.workstation.example -> .env.workstation" -ForegroundColor Yellow
        Copy-Item $envExample $envFile
        Write-Host "[init] 请按需编辑 .env.workstation 后再次执行此命令。" -ForegroundColor Yellow
        exit 0
    }
    throw "未找到 .env.workstation 与 .env.workstation.example"
}

if (-not (Test-Path $configFile) -and $Action -in @("up", "restart", "build", "rebuild")) {
    $runtimeDir = Join-Path $repoRoot "runtime"
    if (-not (Test-Path $runtimeDir)) {
        New-Item -ItemType Directory -Path $runtimeDir | Out-Null
    }
    if (Test-Path $configExample) {
        Write-Host "[init] 复制 config.example.json -> runtime/workstation.config.json" -ForegroundColor Yellow
        Copy-Item $configExample $configFile
        Write-Host "[init] 请按需编辑 runtime/workstation.config.json 后再次执行此命令。" -ForegroundColor Yellow
        exit 0
    }
    throw "未找到 runtime/workstation.config.json 与 config.example.json"
}

$composeArgs = @("compose", "--env-file", $envFile, "-f", $composeFile)

function Invoke-Compose {
    param([string[]]$ExtraArgs)
    $all = $composeArgs + $ExtraArgs
    Write-Host "docker $($all -join ' ')" -ForegroundColor Cyan
    & docker @all
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

switch ($Action) {
    "up" {
        $extra = @("up")
        if ($Detach) { $extra += "-d" }
        $extra += "--build"
        Invoke-Compose $extra
    }
    "down" {
        Invoke-Compose @("down")
    }
    "restart" {
        Invoke-Compose @("restart")
    }
    "build" {
        Invoke-Compose @("build")
    }
    "rebuild" {
        Invoke-Compose @("build", "--no-cache")
        Invoke-Compose @("up", "-d")
    }
    "logs" {
        $extra = @("logs", "--tail=200")
        if ($Follow) { $extra += "-f" }
        Invoke-Compose $extra
    }
    "ps" {
        Invoke-Compose @("ps")
    }
    "shell" {
        Invoke-Compose @("exec", "workstation", "bash")
    }
    "config" {
        Invoke-Compose @("config")
    }
    "test" {
        # 在镜像里跑 pytest；不依赖容器是否在 up 状态
        # 默认跑全量 backend tests；通过位置参数透传 pytest 路径与开关
        $pytestArgs = @("tests")
        if ($Rest -and $Rest.Count -gt 0) {
            $pytestArgs = $Rest
        }
        $extra = @("run", "--rm", "--no-deps", "--entrypoint", "", "workstation", "pytest") + $pytestArgs
        Invoke-Compose $extra
    }
    "exec" {
        if (-not $Rest -or $Rest.Count -eq 0) {
            throw "exec 需要透传命令，例如: ... exec -- python --version"
        }
        $extra = @("run", "--rm", "--no-deps", "--entrypoint", "", "workstation") + $Rest
        Invoke-Compose $extra
    }
    "lint" {
        # 直接用 docker run + bind mount，让 ruff/black 修宿主机源码（compose run 默认不挂源码）。
        $install = "pip install --quiet --no-cache-dir 'ruff==0.8.6' 'black==24.10.0'"
        if ($Fix) {
            # 用 ; 而非 && —— ruff 仍有 unfixed errors 时退出非 0，但仍想继续跑 black format
            $bashLine = "$install ; cd /app && ruff check --fix backend ; black backend"
        } else {
            $bashLine = "$install && cd /app && ruff check backend && black --check backend"
        }
        $env:MSYS_NO_PATHCONV = "1"
        $imageTag = if ($env:ATS_WORKSTATION_IMAGE) { $env:ATS_WORKSTATION_IMAGE } else { "agentthespire/workstation:local" }
        $backendMount = "$($repoRoot.Path)/backend:/app/backend"
        $configMount = "$($repoRoot.Path)/pyproject.toml:/app/pyproject.toml:ro"
        $dockerArgs = @(
            "run", "--rm", "--entrypoint", "",
            "-v", $backendMount,
            "-v", $configMount,
            $imageTag,
            "bash", "-c", $bashLine
        )
        Write-Host "docker $($dockerArgs -join ' ')" -ForegroundColor Cyan
        & docker @dockerArgs
        $env:MSYS_NO_PATHCONV = $null
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
}
