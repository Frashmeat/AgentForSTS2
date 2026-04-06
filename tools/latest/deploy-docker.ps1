<#
.SYNOPSIS
按目标启动 AgentTheSpire 的 Docker 部署。

.DESCRIPTION
读取 release bundle、生成运行时配置并执行 docker compose。
直接执行脚本且不传任何参数时，会默认显示本帮助而不是立即启动部署。

.PARAMETER Target
部署目标。可选 full / workstation / frontend / web。

.PARAMETER ReleaseRoot
release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。

.PARAMETER ConfigPath
运行时配置文件路径。默认读取仓库根目录 config.json。

.PARAMETER ProjectName
Compose 项目名。默认按 agentthespire-<target>-release 生成。

.PARAMETER WorkstationPort
工作站端口。默认 7860。

.PARAMETER WebPort
Web 端口。默认 7870。

.PARAMETER FrontendPort
前端静态站端口。默认 8080。

.PARAMETER PostgresHostPort
Postgres 暴露到宿主机的端口。默认 5432。

.PARAMETER PostgresDb
Postgres 数据库名。默认 agentthespire。

.PARAMETER PostgresUser
Postgres 用户名。默认 agentthespire。

.PARAMETER PostgresPassword
Postgres 密码。默认 agentthespire。

.PARAMETER PostgresImage
Postgres 镜像名。留空时自动优先复用本机已有镜像。

.PARAMETER ResetDatabase
重建数据库。full 默认会执行；web 目标可显式开启。

.PARAMETER ReuseImages
复用已有镜像。仅在镜像缺失时才执行 docker compose build。

.PARAMETER RebuildImages
强制重建镜像。会删除当前项目对应镜像并重新 docker compose build。

.PARAMETER Help
显示帮助说明并退出。

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 workstation

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 web -ResetDb -dbn agentthespire
#>
[CmdletBinding()]
param(
    # 基础参数
    [Parameter(Position = 0, HelpMessage = "部署目标。可选 full / workstation / frontend / web。")]
    [Alias("t")]
    [ValidateSet("full", "workstation", "frontend", "web")]
    [string]$Target = "workstation",

    [Parameter(HelpMessage = "release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。")]
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Parameter(HelpMessage = "运行时配置文件路径。默认读取仓库根目录 config.json。")]
    [Alias("c")]
    [string]$ConfigPath = "",

    [Parameter(HelpMessage = "Compose 项目名。默认按 agentthespire-<target>-release 生成。")]
    [Alias("n")]
    [string]$ProjectName = "",

    # 端口参数
    [Parameter(HelpMessage = "工作站端口。默认 7860。")]
    [Alias("ws")]
    [string]$WorkstationPort = "7860",

    [Parameter(HelpMessage = "Web 端口。默认 7870。")]
    [Alias("wp")]
    [string]$WebPort = "7870",

    [Parameter(HelpMessage = "前端静态站端口。默认 8080。")]
    [Alias("fp")]
    [string]$FrontendPort = "8080",

    # 数据库参数
    [Parameter(HelpMessage = "Postgres 暴露到宿主机的端口。默认 5432。")]
    [Alias("dbp")]
    [string]$PostgresHostPort = "5432",

    [Parameter(HelpMessage = "Postgres 数据库名。默认 agentthespire。")]
    [Alias("dbn")]
    [string]$PostgresDb = "agentthespire",

    [Parameter(HelpMessage = "Postgres 用户名。默认 agentthespire。")]
    [Alias("dbu")]
    [string]$PostgresUser = "agentthespire",

    [Parameter(HelpMessage = "Postgres 密码。默认 agentthespire。")]
    [Alias("dbpw")]
    [string]$PostgresPassword = "agentthespire",

    [Parameter(HelpMessage = "Postgres 镜像名。留空时自动优先复用本机已有镜像。")]
    [Alias("pg")]
    [string]$PostgresImage = "",

    # 行为开关
    [Parameter(HelpMessage = "重建数据库。full 默认会执行；web 目标可显式开启。")]
    [Alias("ResetDb")]
    [switch]$ResetDatabase,

    [Parameter(HelpMessage = "复用已有镜像。仅在镜像缺失时才执行 docker compose build。")]
    [Alias("Reuse")]
    [switch]$ReuseImages,

    [Parameter(HelpMessage = "强制重建镜像。会删除当前项目对应镜像并重新 docker compose build。")]
    [Alias("Rebuild")]
    [switch]$RebuildImages,

    [Parameter(HelpMessage = "显示帮助说明并退出。")]
    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help -or $PSBoundParameters.Count -eq 0) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    $ConfigPath = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path "config.json"
}

if ($ReuseImages.IsPresent -and $RebuildImages.IsPresent) {
    throw "-ReuseImages 与 -RebuildImages 不能同时使用。"
}

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
        [string]$FallbackServiceDir
    )

    if (Test-Path -LiteralPath $PreferredPath) {
        return (Resolve-Path $PreferredPath).Path
    }

    $fallback = Join-Path $FallbackServiceDir "config.example.json"
    if (Test-Path -LiteralPath $fallback) {
        return $fallback
    }

    throw "未找到可用配置文件。请提供 config.json，或先执行打包脚本生成 release bundle。"
}

function Ensure-Hashtable {
    param([object]$Value)

    if ($Value -is [hashtable]) {
        return $Value
    }
    if ($null -eq $Value) {
        return @{}
    }
    return @{} + $Value
}

function New-RuntimeConfig {
    param(
        [string]$SourceConfigPath,
        [ValidateSet("workstation", "web")]
        [string]$Mode,
        [string]$DbUser,
        [string]$DbPassword,
        [string]$DbName
    )

    $config = Get-Content -LiteralPath $SourceConfigPath -Raw | ConvertFrom-Json -AsHashtable
    if (-not $config) {
        $config = @{}
    }

    $config["migration"] = Ensure-Hashtable -Value $config["migration"]
    $config["database"] = Ensure-Hashtable -Value $config["database"]

    if ($Mode -eq "web") {
        # Web 目标始终接管数据库连接，并强制打开平台 API 相关开关。
        $config["database"]["url"] = "postgresql+psycopg://{0}:{1}@postgres:5432/{2}" -f $DbUser, $DbPassword, $DbName
        $config["database"]["echo"] = $false
        $config["database"]["pool_pre_ping"] = $true
        $config["migration"]["platform_jobs_api_enabled"] = $true
        $config["migration"]["platform_service_split_enabled"] = $true
    } else {
        # workstation/full 中的工作站进程不应暴露 web 平台路由。
        $config["migration"]["platform_jobs_api_enabled"] = $false
        $config["migration"]["platform_service_split_enabled"] = $false
    }

    return $config
}

function Write-RuntimeConfigFile {
    param(
        [hashtable]$Config,
        [string]$OutputPath
    )

    $parentDir = Split-Path -Path $OutputPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($parentDir)) {
        $null = New-Item -ItemType Directory -Path $parentDir -Force
    }

    if (Test-Path -LiteralPath $OutputPath -PathType Container) {
        # runtime/*.config.json 属于脚本托管产物，若之前被错误创建成目录则在写入前自愈。
        Remove-Item -LiteralPath $OutputPath -Recurse -Force
    }

    $json = $Config | ConvertTo-Json -Depth 20
    Set-Content -LiteralPath $OutputPath -Value $json -Encoding UTF8
}

function Write-ComposeEnvFile {
    param(
        [string]$TargetName,
        [string]$EnvPath,
        [string]$ResolvedPostgresImage
    )

    # Compose 模板的端口、数据库和镜像选择都从 runtime/.env 注入，bundle 本身保持静态。
    $lines = switch ($TargetName) {
        "full" {
            @(
                "ATS_WORKSTATION_PORT=$WorkstationPort"
                "ATS_WEB_PORT=$WebPort"
                "ATS_POSTGRES_HOST_PORT=$PostgresHostPort"
                "ATS_POSTGRES_DB=$PostgresDb"
                "ATS_POSTGRES_USER=$PostgresUser"
                "ATS_POSTGRES_PASSWORD=$PostgresPassword"
                "ATS_POSTGRES_IMAGE=$ResolvedPostgresImage"
            )
        }
        "workstation" {
            @("ATS_WORKSTATION_PORT=$WorkstationPort")
        }
        "frontend" {
            @("ATS_FRONTEND_PORT=$FrontendPort")
        }
        "web" {
            @(
                "ATS_WEB_PORT=$WebPort"
                "ATS_POSTGRES_HOST_PORT=$PostgresHostPort"
                "ATS_POSTGRES_DB=$PostgresDb"
                "ATS_POSTGRES_USER=$PostgresUser"
                "ATS_POSTGRES_PASSWORD=$PostgresPassword"
                "ATS_POSTGRES_IMAGE=$ResolvedPostgresImage"
            )
        }
        default {
            throw "未知 Target: $TargetName"
        }
    }

    Set-Content -LiteralPath $EnvPath -Value ($lines -join [Environment]::NewLine) -Encoding UTF8
}

function Invoke-DockerCompose {
    param(
        [string]$BundleDir,
        [string]$ComposeFile,
        [string]$EnvFile,
        [string]$ComposeProjectName,
        [string[]]$ComposeArgs
    )

    Push-Location $BundleDir
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

function Get-BuildServices {
    param(
        [ValidateSet("full", "workstation", "frontend", "web")]
        [string]$TargetName
    )

    switch ($TargetName) {
        "full" { return @("workstation", "web") }
        "workstation" { return @("workstation") }
        "frontend" { return @("frontend") }
        "web" { return @("web") }
        default { throw "未知 Target: $TargetName" }
    }
}

function Get-ComposeImageName {
    param(
        [string]$ComposeProjectName,
        [string]$ServiceName
    )

    return "{0}-{1}:latest" -f $ComposeProjectName, $ServiceName
}

function Test-DockerImageExists {
    param([string]$ImageName)

    try {
        & docker image inspect $ImageName 1>$null 2>$null
        return $LASTEXITCODE -eq 0
    }
    catch {
        return $false
    }
}

function Remove-DockerImageIfExists {
    param([string]$ImageName)

    if (Test-DockerImageExists -ImageName $ImageName) {
        & docker image rm -f $ImageName
        if ($LASTEXITCODE -ne 0) {
            throw "删除旧镜像失败: $ImageName"
        }
    }
}

function Resolve-PostgresImage {
    param([string]$PreferredImage)

    if (-not [string]::IsNullOrWhiteSpace($PreferredImage)) {
        return $PreferredImage
    }

    # 优先复用本机已有镜像，减少首次以外的网络拉取；都不存在时再回退到默认 upstream 名称。
    $candidates = @(
        "postgres:16-alpine",
        "m.daocloud.io/docker.io/library/postgres:16-alpine",
        "postgres:15-alpine",
        "m.daocloud.io/docker.io/library/postgres:15-alpine"
    )

    foreach ($candidate in $candidates) {
        if (Test-DockerImageExists -ImageName $candidate) {
            return $candidate
        }
    }

    return "postgres:16-alpine"
}

$effectiveReleaseRoot = if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    Join-Path $PSScriptRoot ("artifacts\agentthespire-{0}-release" -f $Target)
} else {
    $ReleaseRoot
}
$effectiveProjectName = if ([string]::IsNullOrWhiteSpace($ProjectName)) {
    "agentthespire-$Target-release"
} else {
    $ProjectName
}

Assert-CommandExists -CommandName "docker"
Assert-PathExists -Path $effectiveReleaseRoot -Label "release 目录"

$composeFile = Join-Path $effectiveReleaseRoot "docker-compose.yml"
$runtimeDir = Join-Path $effectiveReleaseRoot "runtime"
$envFile = Join-Path $runtimeDir ".env"
$targetNeedsPostgres = $Target -in @("full", "web")
$resolvedPostgresImage = if ($targetNeedsPostgres) {
    Resolve-PostgresImage -PreferredImage $PostgresImage
} else {
    ""
}
$shouldResetDatabase = $Target -eq "full" -or $ResetDatabase.IsPresent

Assert-PathExists -Path $composeFile -Label "docker-compose.yml"
$null = New-Item -ItemType Directory -Path $runtimeDir -Force
Write-ComposeEnvFile -TargetName $Target -EnvPath $envFile -ResolvedPostgresImage $resolvedPostgresImage

if ($Target -in @("full", "workstation", "web")) {
    $serviceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "workstation"
    if ($Target -eq "web") {
        $serviceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "web"
    }
    $sourceConfigPath = Get-SourceConfigPath -PreferredPath $ConfigPath -FallbackServiceDir $serviceDir
}

if ($Target -eq "full" -or $Target -eq "workstation") {
    $workstationConfig = New-RuntimeConfig -SourceConfigPath $sourceConfigPath -Mode "workstation" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
    Write-RuntimeConfigFile -Config $workstationConfig -OutputPath (Join-Path $runtimeDir "workstation.config.json")
}

if ($Target -eq "full" -or $Target -eq "web") {
    if ($Target -eq "full") {
        $webSourceConfigPath = Get-SourceConfigPath -PreferredPath $ConfigPath -FallbackServiceDir (Join-Path (Join-Path $effectiveReleaseRoot "services") "web")
    } else {
        $webSourceConfigPath = $sourceConfigPath
    }
    $webConfig = New-RuntimeConfig -SourceConfigPath $webSourceConfigPath -Mode "web" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
    Write-RuntimeConfigFile -Config $webConfig -OutputPath (Join-Path $runtimeDir "web.config.json")
}

if ($shouldResetDatabase) {
    if ($Target -notin @("full", "web")) {
        throw "-ResetDatabase 仅适用于包含 Web 后端数据库的部署目标: full / web"
    }
    # full 目标默认重建数据库，避免同机联调时复用旧卷里的脏迁移状态。
    $resetReason = if ($Target -eq "full" -and (-not $ResetDatabase.IsPresent)) {
        "检测到 full 目标，默认重建数据库"
    } else {
        "检测到 -ResetDatabase，将删除 Docker 卷并重建数据库"
    }
    Write-Host "$resetReason..."
    Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("down", "--volumes", "--remove-orphans")
}

$buildServices = Get-BuildServices -TargetName $Target
$servicesToBuild = @()

foreach ($serviceName in $buildServices) {
    $imageName = Get-ComposeImageName -ComposeProjectName $effectiveProjectName -ServiceName $serviceName
    if ($RebuildImages) {
        Remove-DockerImageIfExists -ImageName $imageName
        $servicesToBuild += $serviceName
        continue
    }

    if ($ReuseImages) {
        if (-not (Test-DockerImageExists -ImageName $imageName)) {
            $servicesToBuild += $serviceName
        }
        continue
    }

    $servicesToBuild += $serviceName
}

if ($servicesToBuild.Count -gt 0) {
    # 默认以当前 release 为准重建镜像，避免发布目录更新后仍复用旧镜像。
    Write-Host "检测到需要构建本地镜像: $($servicesToBuild -join ', ')"
    Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("build")
}

Write-Host "启动 Docker 部署..."
Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("up", "-d", "--no-build")

Write-Host ""
Write-Host "部署完成:"
Write-Host "  Target       : $Target"
Write-Host "  Release 目录 : $effectiveReleaseRoot"
Write-Host "  Compose Env  : $envFile"
Write-Host "  复用已有镜像 : $($ReuseImages.IsPresent)"
Write-Host "  强制重建镜像 : $($RebuildImages.IsPresent)"
Write-Host "  重建数据库   : $shouldResetDatabase"
if ($targetNeedsPostgres) {
    Write-Host "  Postgres 镜像: $resolvedPostgresImage"
}
switch ($Target) {
    "full" {
        Write-Host "  工作站地址   : http://127.0.0.1:$WorkstationPort"
        Write-Host "  Web 地址     : http://127.0.0.1:$WebPort"
    }
    "workstation" {
        Write-Host "  工作站地址   : http://127.0.0.1:$WorkstationPort"
    }
    "frontend" {
        Write-Host "  前端地址     : http://127.0.0.1:$FrontendPort"
    }
    "web" {
        Write-Host "  Web 地址     : http://127.0.0.1:$WebPort"
    }
}
