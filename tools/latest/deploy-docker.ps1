[CmdletBinding()]
param(
    [string]$ReleaseRoot = (Join-Path $PSScriptRoot "artifacts\agentthespire-release"),
    [string]$ConfigPath = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path "config.json"),
    [string]$ProjectName = "agentthespire-release",
    [string]$AppPort = "7860",
    [string]$PostgresHostPort = "5432",
    [string]$PostgresDb = "agentthespire",
    [string]$PostgresUser = "agentthespire",
    [string]$PostgresPassword = "agentthespire",
    [switch]$ResetDatabase
)

$ErrorActionPreference = "Stop"

function Assert-PathExists {
    param(
        [string]$Path,
        [string]$Label
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "缺少${Label}: $Path"
    }
}

function Assert-CommandExists {
    param([string]$CommandName)

    if (-not (Get-Command $CommandName -ErrorAction SilentlyContinue)) {
        throw "未找到命令: $CommandName"
    }
}

function Get-SourceConfigPath {
    param(
        [string]$PreferredPath,
        [string]$ReleaseDir
    )

    if (Test-Path -LiteralPath $PreferredPath) {
        return (Resolve-Path $PreferredPath).Path
    }

    $fallback = Join-Path $ReleaseDir "config.example.json"
    if (Test-Path -LiteralPath $fallback) {
        return $fallback
    }

    throw "未找到可用配置文件。请提供 config.json，或先执行打包脚本生成 config.example.json。"
}

function Write-RuntimeConfig {
    param(
        [string]$SourceConfigPath,
        [string]$RuntimeConfigPath,
        [string]$DbUser,
        [string]$DbPassword,
        [string]$DbName
    )

    $config = Get-Content -LiteralPath $SourceConfigPath -Raw | ConvertFrom-Json -AsHashtable
    if (-not $config) {
        $config = @{}
    }
    if (-not $config.ContainsKey("database") -or $null -eq $config["database"]) {
        $config["database"] = @{}
    }

    $config["database"]["url"] = "postgresql+psycopg://{0}:{1}@postgres:5432/{2}" -f $DbUser, $DbPassword, $DbName
    $config["database"]["echo"] = $false
    $config["database"]["pool_pre_ping"] = $true

    $json = $config | ConvertTo-Json -Depth 20
    Set-Content -LiteralPath $RuntimeConfigPath -Value $json -Encoding UTF8
}

function Write-ComposeEnvFile {
    param(
        [string]$EnvPath,
        [string]$AppPortValue,
        [string]$PostgresPortValue,
        [string]$DbName,
        [string]$DbUser,
        [string]$DbPassword
    )

    $lines = @(
        "ATS_APP_PORT=$AppPortValue"
        "ATS_POSTGRES_HOST_PORT=$PostgresPortValue"
        "ATS_POSTGRES_DB=$DbName"
        "ATS_POSTGRES_USER=$DbUser"
        "ATS_POSTGRES_PASSWORD=$DbPassword"
    )
    Set-Content -LiteralPath $EnvPath -Value ($lines -join [Environment]::NewLine) -Encoding UTF8
}

function Invoke-DockerCompose {
    param(
        [string]$ReleaseDir,
        [string]$ComposeFile,
        [string]$EnvFile,
        [string]$ComposeProjectName,
        [string[]]$ComposeArgs
    )

    Push-Location $ReleaseDir
    try {
        & docker compose --project-name $ComposeProjectName --env-file $EnvFile -f $ComposeFile @ComposeArgs
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose 执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

Assert-CommandExists -CommandName "docker"
Assert-PathExists -Path $ReleaseRoot -Label "release 目录"

$composeFile = Join-Path $ReleaseRoot "docker-compose.yml"
$runtimeDir = Join-Path $ReleaseRoot "runtime"
$runtimeConfigPath = Join-Path $runtimeDir "config.json"
$envFile = Join-Path $runtimeDir ".env"

Assert-PathExists -Path $composeFile -Label "docker-compose.yml"
$null = New-Item -ItemType Directory -Path $runtimeDir -Force

$sourceConfigPath = Get-SourceConfigPath -PreferredPath $ConfigPath -ReleaseDir $ReleaseRoot
Write-RuntimeConfig -SourceConfigPath $sourceConfigPath -RuntimeConfigPath $runtimeConfigPath -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
Write-ComposeEnvFile -EnvPath $envFile -AppPortValue $AppPort -PostgresPortValue $PostgresHostPort -DbName $PostgresDb -DbUser $PostgresUser -DbPassword $PostgresPassword

if ($ResetDatabase) {
    Write-Host "检测到 -ResetDatabase，将删除 Docker 卷并重建数据库..."
    Invoke-DockerCompose -ReleaseDir $ReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $ProjectName -ComposeArgs @("down", "--volumes", "--remove-orphans")
}

Write-Host "启动 Docker 部署..."
Invoke-DockerCompose -ReleaseDir $ReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $ProjectName -ComposeArgs @("up", "-d", "--build")

Write-Host ""
Write-Host "部署完成:"
Write-Host "  Release 目录 : $ReleaseRoot"
Write-Host "  运行时配置  : $runtimeConfigPath"
Write-Host "  Compose Env  : $envFile"
Write-Host "  访问地址     : http://127.0.0.1:$AppPort"
Write-Host ""
Write-Host "可使用以下命令查看状态:"
Write-Host "  docker compose --project-name $ProjectName --env-file `"$envFile`" -f `"$composeFile`" ps"
