<#
.SYNOPSIS
按唯一主线部署 AgentTheSpire app。

.DESCRIPTION
读取 runtime/agentthespire.config.json，生成所有运行时派生配置，并部署固定拓扑：
本机 frontend + local-workstation，Docker web + web-workstation + postgres。
直接执行脚本会按默认参数部署；传 -DryRun 只生成配置并打印拓扑。

.PARAMETER ReleaseRoot
release 目录。留空时优先识别当前仓库，否则使用 tools/latest/artifacts/agentthespire-app-release。

.PARAMETER ConfigPath
唯一人工配置文件。默认 runtime/agentthespire.config.json。

.PARAMETER RebuildImages
强制重建 Docker 镜像。

.PARAMETER ReuseImages
复用已有 Docker 镜像，缺失时才构建。

.PARAMETER ResetDatabase
删除 Docker Postgres 卷并重建。

.PARAMETER SkipBootstrap
跳过 Web 数据库迁移和默认管理员初始化。仅用于调试部署脚本。

.PARAMETER DryRun
只生成配置并打印将执行的动作。

.PARAMETER NoBrowser
启动后不打开浏览器。

.PARAMETER Help
显示帮助。

.EXAMPLE
pwsh -File .\tools\latest\deploy-app.ps1

.EXAMPLE
pwsh -File .\tools\latest\deploy-app.ps1 -DryRun
#>
[CmdletBinding()]
param(
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Alias("c")]
    [string]$ConfigPath = "",

    [Alias("Rebuild")]
    [switch]$RebuildImages,

    [Alias("Reuse")]
    [switch]$ReuseImages,

    [Alias("ResetDb")]
    [switch]$ResetDatabase,

    [switch]$SkipBootstrap,

    [switch]$DryRun,

    [switch]$NoBrowser,

    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

if ($ReuseImages.IsPresent -and $RebuildImages.IsPresent) {
    throw "-ReuseImages 与 -RebuildImages 不能同时使用。"
}

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Assert-PathExists {
    param([string]$Path, [string]$Label)
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

function ConvertTo-HashtableRecursive {
    param([object]$Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [System.Collections.IDictionary]) {
        $result = @{}
        foreach ($key in $Value.Keys) {
            $result[[string]$key] = ConvertTo-HashtableRecursive -Value $Value[$key]
        }
        return $result
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $result = @{}
        foreach ($property in $Value.PSObject.Properties) {
            $result[$property.Name] = ConvertTo-HashtableRecursive -Value $property.Value
        }
        return $result
    }
    if (($Value -is [System.Collections.IEnumerable]) -and -not ($Value -is [string])) {
        $items = @()
        foreach ($item in $Value) {
            $items += ,(ConvertTo-HashtableRecursive -Value $item)
        }
        return $items
    }
    return $Value
}

function ConvertFrom-JsonAsHashtableCompat {
    param([string]$JsonText)
    $command = Get-Command ConvertFrom-Json -ErrorAction Stop
    if ($command.Parameters.ContainsKey("AsHashtable")) {
        return $JsonText | ConvertFrom-Json -AsHashtable
    }
    return ConvertTo-HashtableRecursive -Value ($JsonText | ConvertFrom-Json)
}

function Read-JsonHashtableFile {
    param([string]$Path)
    return ConvertFrom-JsonAsHashtableCompat -JsonText (Get-Content -LiteralPath $Path -Raw -Encoding UTF8)
}

function Write-JsonFile {
    param([hashtable]$Value, [string]$Path)
    $parent = Split-Path -Path $Path -Parent
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        $null = New-Item -ItemType Directory -Path $parent -Force
    }
    $Value | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Ensure-Hashtable {
    param([object]$Value)
    if ($Value -is [hashtable]) { return $Value }
    if ($null -eq $Value) { return @{} }
    return @{} + $Value
}

function Merge-Hashtable {
    param([hashtable]$Base, [hashtable]$Override)
    $result = @{} + $Base
    foreach ($key in $Override.Keys) {
        if ($result.ContainsKey($key) -and $result[$key] -is [hashtable] -and $Override[$key] -is [hashtable]) {
            $result[$key] = Merge-Hashtable -Base $result[$key] -Override $Override[$key]
        } else {
            $result[$key] = $Override[$key]
        }
    }
    return $result
}

function New-RandomHexSecret {
    param([int]$ByteCount = 48)
    $bytes = New-Object byte[] $ByteCount
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $rng.GetBytes($bytes)
    } finally {
        if ($null -ne $rng) { $rng.Dispose() }
    }
    return ([System.BitConverter]::ToString($bytes)).Replace("-", "").ToLowerInvariant()
}

function Get-LoopbackOrigins {
    param([int[]]$Ports)
    $origins = New-Object System.Collections.Generic.List[string]
    foreach ($port in $Ports) {
        foreach ($origin in @("http://localhost:$port", "http://127.0.0.1:$port")) {
            if (-not $origins.Contains($origin)) {
                $origins.Add($origin) | Out-Null
            }
        }
    }
    return [string[]]$origins.ToArray()
}

function Get-DefaultAppConfig {
    return @{
        deployment = @{
            frontend_port = 8080
            local_workstation_port = 7860
            web_port = 7870
            postgres_host_port = 55432
            web_base_url = "http://127.0.0.1:7870"
        }
        local_workstation = @{
            config = @{}
        }
        web = @{
            database = @{
                name = "agentthespire"
                user = "agentthespire"
                password = "agentthespire"
            }
            auth = @{
                session_secret = ""
                credential_secret = ""
            }
            config = @{}
        }
        web_workstation = @{
            internal_url = "http://web-workstation:7860"
            control_token = ""
            config = @{}
        }
        docker = @{
            project_name = "agentthespire-app"
            network = "agentthespire-app"
            python_base_image = "python:3.11-slim"
            node_base_image = "node:20-alpine"
            postgres_image = "postgres:16-alpine"
        }
    }
}

function Ensure-AppConfig {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) {
        return Read-JsonHashtableFile -Path $Path
    }
    $default = Get-DefaultAppConfig
    Write-JsonFile -Value $default -Path $Path
    Write-Host "已创建唯一配置文件: $Path"
    return $default
}

function Normalize-AppConfig {
    param([hashtable]$Config)
    return Merge-Hashtable -Base (Get-DefaultAppConfig) -Override $Config
}

function Ensure-AppSecrets {
    param([hashtable]$Config, [string]$Path)

    $changed = $false

    $web = Ensure-Hashtable -Value $Config.web
    $auth = Ensure-Hashtable -Value $web.auth
    foreach ($key in @("session_secret", "credential_secret")) {
        if ([string]::IsNullOrWhiteSpace([string]$auth[$key])) {
            $auth[$key] = New-RandomHexSecret
            $changed = $true
        }
    }
    $web.auth = $auth
    $Config.web = $web

    $webWorkstation = Ensure-Hashtable -Value $Config.web_workstation
    if ([string]::IsNullOrWhiteSpace([string]$webWorkstation.control_token)) {
        $webWorkstation.control_token = New-RandomHexSecret
        $Config.web_workstation = $webWorkstation
        $changed = $true
    }

    if ($changed) {
        Write-JsonFile -Value $Config -Path $Path
    }
    return $Config
}

function Resolve-AppLayout {
    param([string]$PreferredReleaseRoot)

    $repoRoot = Get-RepoRoot
    if ([string]::IsNullOrWhiteSpace($PreferredReleaseRoot)) {
        if (Test-Path -LiteralPath (Join-Path $repoRoot "services\local-workstation\backend\main_workstation.py")) {
            return @{
                Mode = "release"
                Root = $repoRoot
                RepoRoot = $repoRoot
                ComposeFile = Join-Path $repoRoot "docker-compose.yml"
                LocalBackend = Join-Path $repoRoot "services\local-workstation\backend"
                LocalFrontendDist = Join-Path $repoRoot "services\frontend\frontend\dist"
                ConfigRoot = Join-Path $repoRoot "runtime"
                WebContext = Join-Path $repoRoot "services\web"
                WebDockerfile = "Dockerfile"
                WebWorkstationContext = Join-Path $repoRoot "services\web-workstation"
                WebWorkstationDockerfile = "Dockerfile"
            }
        }
        if (Test-Path -LiteralPath (Join-Path $repoRoot "backend\main_workstation.py")) {
            return @{
                Mode = "repo"
                Root = $repoRoot
                RepoRoot = $repoRoot
                ComposeFile = Join-Path $repoRoot "tools\latest\templates\compose.app.yml"
                LocalBackend = Join-Path $repoRoot "backend"
                LocalFrontendDist = Join-Path $repoRoot "frontend\dist"
                ConfigRoot = Join-Path $repoRoot "runtime"
                WebContext = $repoRoot
                WebDockerfile = "docker/Dockerfile.web"
                WebWorkstationContext = $repoRoot
                WebWorkstationDockerfile = "docker/Dockerfile.workstation"
            }
        }
        $PreferredReleaseRoot = Join-Path $PSScriptRoot "artifacts\agentthespire-app-release"
    }

    $releaseRoot = (Resolve-Path $PreferredReleaseRoot).Path
    return @{
        Mode = "release"
        Root = $releaseRoot
        RepoRoot = $repoRoot
        ComposeFile = Join-Path $releaseRoot "docker-compose.yml"
        LocalBackend = Join-Path $releaseRoot "services\local-workstation\backend"
        LocalFrontendDist = Join-Path $releaseRoot "services\frontend\frontend\dist"
        ConfigRoot = Join-Path $releaseRoot "runtime"
        WebContext = Join-Path $releaseRoot "services\web"
        WebDockerfile = "Dockerfile"
        WebWorkstationContext = Join-Path $releaseRoot "services\web-workstation"
        WebWorkstationDockerfile = "Dockerfile"
    }
}

function Get-ConfigPath {
    param([hashtable]$Layout, [string]$PreferredPath)
    if (-not [string]::IsNullOrWhiteSpace($PreferredPath)) {
        return $PreferredPath
    }
    return Join-Path $Layout.ConfigRoot "agentthespire.config.json"
}

function New-RoleConfig {
    param(
        [hashtable]$AppConfig,
        [ValidateSet("local-workstation", "web", "web-workstation")]
        [string]$Role
    )

    $deployment = Ensure-Hashtable -Value $AppConfig.deployment
    $frontendPort = [int]$deployment.frontend_port
    $localWorkstationPort = [int]$deployment.local_workstation_port
    $webPort = [int]$deployment.web_port
    $cors = Get-LoopbackOrigins -Ports @($frontendPort, $localWorkstationPort, $webPort)
    $configExamplePath = if ($Role -eq "web") {
        Join-Path $script:ActiveLayout.WebContext "config.example.json"
    } elseif ($Role -eq "web-workstation") {
        Join-Path $script:ActiveLayout.WebWorkstationContext "config.example.json"
    } else {
        $candidate = Join-Path (Split-Path -Parent $script:ActiveLayout.LocalBackend) "config.example.json"
        if (Test-Path -LiteralPath $candidate) { $candidate } else { Join-Path (Get-RepoRoot) "config.example.json" }
    }
    $base = Read-JsonHashtableFile -Path $configExamplePath
    $base.runtime = Ensure-Hashtable -Value $base.runtime
    $base.runtime.workstation = Ensure-Hashtable -Value $base.runtime.workstation
    $base.runtime.web = Ensure-Hashtable -Value $base.runtime.web
    $base.runtime.workstation.cors_origins = $cors
    $base.runtime.workstation.allow_loopback_origins = $true
    $base.runtime.web.cors_origins = $cors
    $base.runtime.web.allow_loopback_origins = $true
    $base.platform_execution = Ensure-Hashtable -Value $base.platform_execution
    $base.platform_execution.control_token_env = "ATS_WORKSTATION_CONTROL_TOKEN"

    if ($Role -eq "web") {
        $web = Ensure-Hashtable -Value $AppConfig.web
        $db = Ensure-Hashtable -Value $web.database
        $auth = Ensure-Hashtable -Value $web.auth
        $base.database = Ensure-Hashtable -Value $base.database
        $base.database.url = "postgresql+psycopg://{0}:{1}@postgres:5432/{2}" -f $db.user, $db.password, $db.name
        $base.database.echo = $false
        $base.database.pool_pre_ping = $true
        $base.auth = Ensure-Hashtable -Value $base.auth
        $base.auth.session_secret = [string]$auth.session_secret
        $base.platform_execution.workstation_url = [string](Ensure-Hashtable -Value $AppConfig.web_workstation).internal_url
        $base.platform_execution.workstation_config_path = "/app/runtime/generated/web-workstation.config.json"
        $base.platform_execution.auto_start = $false
        $base.migration = @{
            platform_jobs_api_enabled = $true
            platform_service_split_enabled = $true
        }
        return Merge-Hashtable -Base $base -Override (Ensure-Hashtable -Value $web.config)
    }

    $base.migration = @{
        platform_jobs_api_enabled = $false
        platform_service_split_enabled = $false
    }
    $base.platform_execution.auto_start = $false
    $base.platform_execution.workstation_url = if ($Role -eq "web-workstation") {
        [string](Ensure-Hashtable -Value $AppConfig.web_workstation).internal_url
    } else {
        "http://127.0.0.1:{0}" -f $localWorkstationPort
    }

    if ($Role -eq "local-workstation") {
        return Merge-Hashtable -Base $base -Override (Ensure-Hashtable -Value (Ensure-Hashtable -Value $AppConfig.local_workstation).config)
    }
    return Merge-Hashtable -Base $base -Override (Ensure-Hashtable -Value (Ensure-Hashtable -Value $AppConfig.web_workstation).config)
}

function Write-RuntimeConfigJs {
    param([hashtable]$AppConfig, [string]$OutputPath)
    $deployment = Ensure-Hashtable -Value $AppConfig.deployment
    $localWorkstationPort = [int]$deployment.local_workstation_port
    $webBaseUrl = [string]$deployment.web_base_url
    if ([string]::IsNullOrWhiteSpace($webBaseUrl)) {
        $webBaseUrl = "http://127.0.0.1:{0}" -f ([int]$deployment.web_port)
    }
    $parent = Split-Path -Path $OutputPath -Parent
    $null = New-Item -ItemType Directory -Path $parent -Force
    @"
window.__AGENT_THE_SPIRE_API_BASES__ = {
  workstation: "http://127.0.0.1:$localWorkstationPort",
  web: "$webBaseUrl"
};

window.__AGENT_THE_SPIRE_WS_BASES__ = {
  workstation: "ws://127.0.0.1:$localWorkstationPort"
};
"@ | Set-Content -LiteralPath $OutputPath -Encoding UTF8
}

function Convert-PathForComposeEnv {
    param([string]$Path)
    return ([System.IO.Path]::GetFullPath($Path) -replace "\\", "/")
}

function Write-DockerEnv {
    param([hashtable]$AppConfig, [hashtable]$Layout, [string]$OutputPath, [hashtable]$GeneratedPaths)
    $deployment = Ensure-Hashtable -Value $AppConfig.deployment
    $docker = Ensure-Hashtable -Value $AppConfig.docker
    $web = Ensure-Hashtable -Value $AppConfig.web
    $db = Ensure-Hashtable -Value $web.database
    $auth = Ensure-Hashtable -Value $web.auth
    $webWorkstation = Ensure-Hashtable -Value $AppConfig.web_workstation
    $localWorkstationConfig = New-RoleConfig -AppConfig $AppConfig -Role "local-workstation"
    $llm = Ensure-Hashtable -Value $localWorkstationConfig.llm
    $imageGen = Ensure-Hashtable -Value $localWorkstationConfig.image_gen
    $lines = @(
        "ATS_WEB_PORT=$($deployment.web_port)"
        "ATS_POSTGRES_HOST_PORT=$($deployment.postgres_host_port)"
        "ATS_POSTGRES_DB=$($db.name)"
        "ATS_POSTGRES_USER=$($db.user)"
        "ATS_POSTGRES_PASSWORD=$($db.password)"
        "ATS_POSTGRES_IMAGE=$($docker.postgres_image)"
        "ATS_DOCKER_NETWORK=$($docker.network)"
        "ATS_PYTHON_BASE_IMAGE=$($docker.python_base_image)"
        "ATS_NODE_BASE_IMAGE=$($docker.node_base_image)"
        "SPIREFORGE_AUTH_SESSION_SECRET=$($auth.session_secret)"
        "SPIREFORGE_SERVER_CREDENTIAL_SECRET=$($auth.credential_secret)"
        "ATS_WORKSTATION_CONTROL_TOKEN=$($webWorkstation.control_token)"
        "SPIREFORGE_LLM_KEY=$($llm.api_key)"
        "SPIREFORGE_IMG_KEY=$($imageGen.api_key)"
        "SPIREFORGE_IMG_SECRET=$($imageGen.api_secret)"
        "ATS_WEB_CONTEXT=$(Convert-PathForComposeEnv -Path $Layout.WebContext)"
        "ATS_WEB_DOCKERFILE=$($Layout.WebDockerfile)"
        "ATS_WEB_WORKSTATION_CONTEXT=$(Convert-PathForComposeEnv -Path $Layout.WebWorkstationContext)"
        "ATS_WEB_WORKSTATION_DOCKERFILE=$($Layout.WebWorkstationDockerfile)"
        "ATS_WEB_CONFIG_PATH=$(Convert-PathForComposeEnv -Path $GeneratedPaths.WebConfig)"
        "ATS_WEB_RUNTIME_DIR=$(Convert-PathForComposeEnv -Path (Join-Path $Layout.ConfigRoot 'web'))"
        "ATS_WEB_WORKSTATION_CONFIG_PATH=$(Convert-PathForComposeEnv -Path $GeneratedPaths.WebWorkstationConfig)"
        "ATS_WEB_WORKSTATION_RUNTIME_DIR=$(Convert-PathForComposeEnv -Path (Join-Path $Layout.ConfigRoot 'web-workstation'))"
    )
    $parent = Split-Path -Path $OutputPath -Parent
    $null = New-Item -ItemType Directory -Path $parent -Force
    Set-Content -LiteralPath $OutputPath -Value ($lines -join [Environment]::NewLine) -Encoding UTF8
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
    foreach ($pidValue in $pids) {
        try { Stop-Process -Id ([int]$pidValue) -Force -ErrorAction Stop } catch {}
    }
}

function Resolve-PythonCommand {
    param([string]$BackendRoot, [string]$RepoRoot)
    foreach ($candidate in @(
        (Join-Path $BackendRoot ".venv\Scripts\python.exe"),
        (Join-Path $RepoRoot "backend\.venv\Scripts\python.exe")
    )) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $python) { return $python.Source }
    throw "未找到可用 Python。"
}

function Start-LocalProcess {
    param(
        [string]$ServiceName,
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory,
        [hashtable]$Environment,
        [string]$LogRoot
    )
    $null = New-Item -ItemType Directory -Path $LogRoot -Force
    $stdout = Join-Path $LogRoot ("{0}.stdout.log" -f $ServiceName)
    $stderr = Join-Path $LogRoot ("{0}.stderr.log" -f $ServiceName)
    foreach ($key in $Environment.Keys) {
        [Environment]::SetEnvironmentVariable([string]$key, [string]$Environment[$key], "Process")
    }
    return Start-Process -FilePath $FilePath -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory -RedirectStandardOutput $stdout -RedirectStandardError $stderr -WindowStyle Hidden -PassThru
}

function Write-LocalState {
    param([hashtable]$Layout, [array]$Processes)
    $statePath = Join-Path $Layout.ConfigRoot "app-deploy-state.json"
    $payload = @{
        target = "app"
        processes = @($Processes | ForEach-Object {
            @{
                service_name = $_.ServiceName
                pid = $_.Process.Id
                port = $_.Port
            }
        })
        updated_at = (Get-Date).ToString("o")
    }
    $payload | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $statePath -Encoding UTF8
}

function Invoke-DockerCompose {
    param([hashtable]$AppConfig, [hashtable]$Layout, [string]$EnvFile, [string[]]$ComposeArgs)
    $projectName = [string](Ensure-Hashtable -Value $AppConfig.docker).project_name
    Push-Location $Layout.Root
    try {
        $dockerArgs = @("compose", "--project-name", $projectName, "--env-file", $EnvFile, "-f", $Layout.ComposeFile) + $ComposeArgs
        Write-Host ("Docker compose: docker {0}" -f ($dockerArgs -join " "))
        & docker @dockerArgs
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose 执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

function Assert-DockerComposeServicesRunning {
    param([hashtable]$AppConfig, [hashtable]$Layout, [string]$EnvFile)

    $projectName = [string](Ensure-Hashtable -Value $AppConfig.docker).project_name
    $expectedServices = @("postgres", "web-workstation", "web")
    Push-Location $Layout.Root
    try {
        foreach ($service in $expectedServices) {
            $containerId = (& docker compose --project-name $projectName --env-file $EnvFile -f $Layout.ComposeFile ps -q $service).Trim()
            if ($LASTEXITCODE -ne 0) {
                throw "docker compose ps $service 执行失败，退出码: $LASTEXITCODE"
            }
            if ([string]::IsNullOrWhiteSpace($containerId)) {
                throw "Docker web 栈未部署完整：缺少服务 $service。请检查 docker compose 输出。"
            }

            $state = (& docker inspect -f "{{.State.Status}}" $containerId).Trim()
            if ($LASTEXITCODE -ne 0) {
                throw "docker inspect $service 执行失败，退出码: $LASTEXITCODE"
            }
            if ($state -ne "running") {
                $logsCommand = "docker compose --project-name $projectName --env-file $EnvFile -f $($Layout.ComposeFile) logs $service"
                throw "Docker web 栈服务 $service 当前状态为 $state，未成功运行。请执行 $logsCommand 查看日志。"
            }
        }
    }
    finally {
        Pop-Location
    }
}

function Invoke-DockerComposeExec {
    param([hashtable]$AppConfig, [hashtable]$Layout, [string]$EnvFile, [string[]]$ExecArgs)
    $projectName = [string](Ensure-Hashtable -Value $AppConfig.docker).project_name
    Push-Location $Layout.Root
    try {
        $dockerArgs = @("compose", "--project-name", $projectName, "--env-file", $EnvFile, "-f", $Layout.ComposeFile, "exec", "-T") + $ExecArgs
        Write-Host ("Docker compose exec: docker {0}" -f ($dockerArgs -join " "))
        & docker @dockerArgs
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose exec 执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

function Invoke-WebRuntimeBootstrap {
    param([hashtable]$AppConfig, [hashtable]$Layout, [string]$EnvFile)

    Write-Host "执行 Web 数据库迁移"
    Invoke-DockerComposeExec -AppConfig $AppConfig -Layout $Layout -EnvFile $EnvFile -ExecArgs @("web", "alembic", "upgrade", "head")

    Write-Host "确保默认管理员账号存在"
    Invoke-DockerComposeExec -AppConfig $AppConfig -Layout $Layout -EnvFile $EnvFile -ExecArgs @("web", "python", "tools/bootstrap_web_runtime.py", "--ensure-default-admin")
}

$layout = Resolve-AppLayout -PreferredReleaseRoot $ReleaseRoot
$script:ActiveLayout = $layout
Assert-PathExists -Path $layout.ComposeFile -Label "app compose 模板"
Assert-PathExists -Path $layout.LocalBackend -Label "local-workstation backend"
if (-not $DryRun) {
    Assert-PathExists -Path $layout.LocalFrontendDist -Label "frontend/dist"
}

$effectiveConfigPath = Get-ConfigPath -Layout $layout -PreferredPath $ConfigPath
$null = New-Item -ItemType Directory -Path $layout.ConfigRoot -Force
$appConfig = Normalize-AppConfig -Config (Ensure-AppConfig -Path $effectiveConfigPath)
$appConfig = Ensure-AppSecrets -Config $appConfig -Path $effectiveConfigPath

$generatedDir = Join-Path $layout.ConfigRoot "generated"
$paths = @{
    LocalConfig = Join-Path $generatedDir "local-workstation.config.json"
    WebConfig = Join-Path $generatedDir "web.config.json"
    WebWorkstationConfig = Join-Path $generatedDir "web-workstation.config.json"
    RuntimeConfig = Join-Path $generatedDir "runtime-config.js"
    DockerEnv = Join-Path $generatedDir "docker.env"
}

Write-JsonFile -Value (New-RoleConfig -AppConfig $appConfig -Role "local-workstation") -Path $paths.LocalConfig
Write-JsonFile -Value (New-RoleConfig -AppConfig $appConfig -Role "web") -Path $paths.WebConfig
Write-JsonFile -Value (New-RoleConfig -AppConfig $appConfig -Role "web-workstation") -Path $paths.WebWorkstationConfig
Write-RuntimeConfigJs -AppConfig $appConfig -OutputPath $paths.RuntimeConfig
if (-not $DryRun) {
    Copy-Item -LiteralPath $paths.RuntimeConfig -Destination (Join-Path $layout.LocalFrontendDist "runtime-config.js") -Force
}
Write-DockerEnv -AppConfig $appConfig -Layout $layout -OutputPath $paths.DockerEnv -GeneratedPaths $paths

$deployment = Ensure-Hashtable -Value $appConfig.deployment
$frontendPort = [int]$deployment.frontend_port
$localWorkstationPort = [int]$deployment.local_workstation_port
$webPort = [int]$deployment.web_port

Write-Host ""
Write-Host "部署拓扑:"
Write-Host "  Mode              : $($layout.Mode)"
Write-Host "  Config            : $effectiveConfigPath"
Write-Host "  Generated         : $generatedDir"
Write-Host "  Frontend          : http://127.0.0.1:$frontendPort"
Write-Host "  Local Workstation : http://127.0.0.1:$localWorkstationPort"
Write-Host "  Web               : http://127.0.0.1:$webPort"
Write-Host "  Web Workstation   : $((Ensure-Hashtable -Value $appConfig.web_workstation).internal_url) (Docker 内网)"

if ($DryRun) {
    Write-Host "DryRun：已生成配置，不启动进程或 Docker。"
    return
}

Assert-CommandExists -CommandName "docker"
$python = Resolve-PythonCommand -BackendRoot $layout.LocalBackend -RepoRoot $layout.RepoRoot
$logRoot = Join-Path $layout.ConfigRoot "logs"
$processes = @()

if ($ResetDatabase) {
    Invoke-DockerCompose -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv -ComposeArgs @("down", "--volumes", "--remove-orphans")
}

if ($RebuildImages) {
    Invoke-DockerCompose -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv -ComposeArgs @("build", "--no-cache")
} elseif (-not $ReuseImages) {
    Invoke-DockerCompose -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv -ComposeArgs @("build")
}
Invoke-DockerCompose -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv -ComposeArgs @("up", "-d", "--no-build")
Assert-DockerComposeServicesRunning -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv
if (-not $SkipBootstrap) {
    Invoke-WebRuntimeBootstrap -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv
}

Stop-ProcessListeningOnPort -Port $localWorkstationPort
Stop-ProcessListeningOnPort -Port $frontendPort

$workstationProcess = Start-LocalProcess -ServiceName "local-workstation" -FilePath $python -Arguments @(
    "-m", "uvicorn", "main_workstation:app", "--host", "127.0.0.1", "--port", "$localWorkstationPort"
) -WorkingDirectory $layout.LocalBackend -Environment @{
    SPIREFORGE_CONFIG_PATH = $paths.LocalConfig
    SPIREFORGE_RUNTIME_DIR = $layout.ConfigRoot
} -LogRoot $logRoot
$processes += [pscustomobject]@{ ServiceName = "local-workstation"; Process = $workstationProcess; Port = $localWorkstationPort }

$frontendProcess = Start-LocalProcess -ServiceName "frontend" -FilePath $python -Arguments @(
    "-m", "http.server", "$frontendPort", "--bind", "127.0.0.1", "--directory", $layout.LocalFrontendDist
) -WorkingDirectory $layout.LocalFrontendDist -Environment @{} -LogRoot $logRoot
$processes += [pscustomobject]@{ ServiceName = "frontend"; Process = $frontendProcess; Port = $frontendPort }

Write-LocalState -Layout $layout -Processes $processes

Write-Host ""
Write-Host "部署完成:"
Write-Host "  前端地址     : http://127.0.0.1:$frontendPort"
Write-Host "  工作站地址   : http://127.0.0.1:$localWorkstationPort"
Write-Host "  Web 地址     : http://127.0.0.1:$webPort"
Write-Host "  Docker web 栈: postgres / web-workstation / web 已启动"
if (-not $SkipBootstrap) {
    Write-Host "  默认管理员   : admin / admin@example.com / admin123456（每次部署收敛）"
}
Write-Host "  日志入口     : powershell -File .\tools\tools.ps1 logs app"
Write-Host "  日志目录     : $logRoot"
Write-Host "  停止入口     : powershell -File .\tools\tools.ps1 stop app"

if (-not $NoBrowser) {
    Start-Process ("http://127.0.0.1:{0}" -f $frontendPort) | Out-Null
}
