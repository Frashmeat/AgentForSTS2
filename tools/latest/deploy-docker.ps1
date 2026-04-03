[CmdletBinding()]
param(
    [ValidateSet("full", "workstation", "frontend", "web")]
    [string]$Target = "workstation",
    [string]$ReleaseRoot = "",
    [string]$ConfigPath = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path "config.json"),
    [string]$ProjectName = "",
    [string]$WorkstationPort = "7860",
    [string]$WebPort = "7870",
    [string]$FrontendPort = "8080",
    [string]$PostgresHostPort = "5432",
    [string]$PostgresDb = "agentthespire",
    [string]$PostgresUser = "agentthespire",
    [string]$PostgresPassword = "agentthespire",
    [switch]$ResetDatabase,
    [switch]$RebuildImages
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
        $config["database"]["url"] = "postgresql+psycopg://{0}:{1}@postgres:5432/{2}" -f $DbUser, $DbPassword, $DbName
        $config["database"]["echo"] = $false
        $config["database"]["pool_pre_ping"] = $true
        $config["migration"]["platform_jobs_api_enabled"] = $true
        $config["migration"]["platform_service_split_enabled"] = $true
    } else {
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

    $json = $Config | ConvertTo-Json -Depth 20
    Set-Content -LiteralPath $OutputPath -Value $json -Encoding UTF8
}

function Write-ComposeEnvFile {
    param(
        [string]$TargetName,
        [string]$EnvPath
    )

    $lines = switch ($TargetName) {
        "full" {
            @(
                "ATS_WORKSTATION_PORT=$WorkstationPort"
                "ATS_WEB_PORT=$WebPort"
                "ATS_POSTGRES_HOST_PORT=$PostgresHostPort"
                "ATS_POSTGRES_DB=$PostgresDb"
                "ATS_POSTGRES_USER=$PostgresUser"
                "ATS_POSTGRES_PASSWORD=$PostgresPassword"
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

    & docker image inspect $ImageName *> $null
    return $LASTEXITCODE -eq 0
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

Assert-PathExists -Path $composeFile -Label "docker-compose.yml"
$null = New-Item -ItemType Directory -Path $runtimeDir -Force
Write-ComposeEnvFile -TargetName $Target -EnvPath $envFile

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

if ($ResetDatabase) {
    if ($Target -notin @("full", "web")) {
        throw "-ResetDatabase 仅适用于包含 Web 后端数据库的部署目标: full / web"
    }
    Write-Host "检测到 -ResetDatabase，将删除 Docker 卷并重建数据库..."
    Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("down", "--volumes", "--remove-orphans")
}

$buildServices = Get-BuildServices -TargetName $Target
$missingBuildServices = @()

foreach ($serviceName in $buildServices) {
    $imageName = Get-ComposeImageName -ComposeProjectName $effectiveProjectName -ServiceName $serviceName
    if ($RebuildImages) {
        Remove-DockerImageIfExists -ImageName $imageName
        $missingBuildServices += $serviceName
        continue
    }
    if (-not (Test-DockerImageExists -ImageName $imageName)) {
        $missingBuildServices += $serviceName
    }
}

if ($missingBuildServices.Count -gt 0) {
    Write-Host "检测到需要构建本地镜像: $($missingBuildServices -join ', ')"
    Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("build")
}

Write-Host "启动 Docker 部署..."
Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("up", "-d", "--no-build")

Write-Host ""
Write-Host "部署完成:"
Write-Host "  Target       : $Target"
Write-Host "  Release 目录 : $effectiveReleaseRoot"
Write-Host "  Compose Env  : $envFile"
Write-Host "  强制重建镜像 : $($RebuildImages.IsPresent)"
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
