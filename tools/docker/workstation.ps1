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
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet("up", "down", "restart", "build", "rebuild", "logs", "ps", "shell", "config", "test", "exec")]
    [string]$Action = "up",

    [switch]$Detach = $true,
    [switch]$Follow,

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
}
