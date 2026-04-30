<#
.SYNOPSIS
AgentTheSpire Web 端 Docker 启停脚手架（web 后端 + Postgres + 前端 SPA）。

.DESCRIPTION
包装 docker compose，固定使用 docker-compose.web.yml + .env.web。
首次使用先复制 .env.web.example -> .env.web 并填好 SPIREFORGE_AUTH_SESSION_SECRET。

.PARAMETER Action
动作。可选: up / down / restart / build / rebuild / logs / ps / shell / migrate / reset-db / config

.PARAMETER Service
logs / shell 指定的服务。默认 web；可选 web / frontend / postgres。

.PARAMETER Follow
logs 时使用 -f 持续跟随。

.EXAMPLE
pwsh -File .\tools\docker\web.ps1 up

.EXAMPLE
pwsh -File .\tools\docker\web.ps1 logs -Service postgres -Follow

.EXAMPLE
pwsh -File .\tools\docker\web.ps1 reset-db
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet("up", "down", "restart", "build", "rebuild", "logs", "ps", "shell", "migrate", "reset-db", "config")]
    [string]$Action = "up",

    [Parameter(Position = 1)]
    [ValidateSet("web", "frontend", "postgres")]
    [string]$Service = "web",

    [switch]$Detach = $true,
    [switch]$Follow
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$composeFile = Join-Path $repoRoot "docker-compose.web.yml"
$envFile     = Join-Path $repoRoot ".env.web"
$envExample  = Join-Path $repoRoot ".env.web.example"
$configFile  = Join-Path $repoRoot "runtime\web.config.json"
$configExample = Join-Path $repoRoot "config.example.json"

if (-not (Test-Path $composeFile)) {
    throw "未找到 $composeFile"
}

if (-not (Test-Path $envFile)) {
    if (Test-Path $envExample) {
        Write-Host "[init] 复制 .env.web.example -> .env.web" -ForegroundColor Yellow
        Copy-Item $envExample $envFile
        Write-Host "[init] 必须填写 SPIREFORGE_AUTH_SESSION_SECRET 后再次执行此命令。" -ForegroundColor Yellow
        exit 0
    }
    throw "未找到 .env.web 与 .env.web.example"
}

if (-not (Test-Path $configFile) -and $Action -in @("up", "restart", "build", "rebuild", "migrate")) {
    $runtimeDir = Join-Path $repoRoot "runtime"
    if (-not (Test-Path $runtimeDir)) {
        New-Item -ItemType Directory -Path $runtimeDir | Out-Null
    }
    if (Test-Path $configExample) {
        Write-Host "[init] 复制 config.example.json -> runtime/web.config.json" -ForegroundColor Yellow
        Copy-Item $configExample $configFile
        Write-Host "[init] 编辑 runtime/web.config.json，至少配置 database.url 与 auth.session_secret 后再执行。" -ForegroundColor Yellow
        exit 0
    }
    throw "未找到 runtime/web.config.json 与 config.example.json"
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
        $extra = @("logs", "--tail=200", $Service)
        if ($Follow) { $extra += "-f" }
        Invoke-Compose $extra
    }
    "ps" {
        Invoke-Compose @("ps")
    }
    "shell" {
        # web 镜像基于 python:3.11-slim 自带 bash；frontend(nginx) 与 postgres(alpine) 只有 sh
        $cmd = if ($Service -eq "web") { "bash" } else { "sh" }
        Invoke-Compose @("exec", $Service, $cmd)
    }
    "migrate" {
        Invoke-Compose @("exec", "web", "alembic", "upgrade", "head")
    }
    "reset-db" {
        $confirm = Read-Host "将删除并重建 Postgres 数据卷 ats_postgres_data。确认输入 RESET"
        if ($confirm -ne "RESET") {
            Write-Host "已取消。" -ForegroundColor Yellow
            return
        }
        Invoke-Compose @("down", "-v")
        Invoke-Compose @("up", "-d", "--build")
    }
    "config" {
        Invoke-Compose @("config")
    }
}
