param(
    [string]$ProjectName = "agentthespire-web-release",
    [string]$ReleaseRoot = "",
    [string]$ComposeFile = "",
    [string]$EnvFile = "",
    [string]$PostgresService = "postgres",
    [string]$WebService = "web",
    [string]$DatabaseName = "",
    [string]$DatabaseUser = "",
    [string]$DatabasePassword = "",
    [string]$SeedSql = "..\migrations\sql\2026-03-31-seed-platform-test-data.sql",
    [int]$WaitSeconds = 30,
    [switch]$Yes
)

$ErrorActionPreference = "Stop"

function Resolve-ScriptPath {
    param([string]$RelativePath)

    return (Resolve-Path (Join-Path $PSScriptRoot $RelativePath)).Path
}

function Get-ContainerName {
    param(
        [string]$ComposeProject,
        [string]$BundleDir,
        [string]$ComposePath,
        [string]$ComposeEnvPath,
        [string]$Service
    )

    $dockerArgs = @("compose", "--project-name", $ComposeProject)
    if ((-not [string]::IsNullOrWhiteSpace($ComposeEnvPath)) -and (Test-Path -LiteralPath $ComposeEnvPath)) {
        $dockerArgs += @("--env-file", $ComposeEnvPath)
    }
    if (-not [string]::IsNullOrWhiteSpace($ComposePath)) {
        $dockerArgs += @("-f", $ComposePath)
    }
    $dockerArgs += @("ps", "-q", $Service)

    if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
        Push-Location $BundleDir
    }
    try {
        $containerName = docker @dockerArgs
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($containerName)) {
            throw "未找到 Docker Compose 服务容器: project=$ComposeProject service=$Service"
        }

        return $containerName.Trim()
    }
    finally {
        if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
            Pop-Location
        }
    }
}

function Get-ContainerEnv {
    param(
        [string]$Container,
        [string]$Name
    )

    $value = docker exec $Container printenv $Name 2>$null
    if ($LASTEXITCODE -ne 0) {
        return ""
    }

    return ($value -join "`n").Trim()
}

function Wait-PostgresReady {
    param(
        [string]$Container,
        [string]$User,
        [string]$Database,
        [int]$TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        docker exec $Container pg_isready -U $User -d $Database | Out-Null
        if ($LASTEXITCODE -eq 0) {
            return
        }
        Start-Sleep -Seconds 1
    }

    docker exec $Container pg_isready -U $User -d $Database | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "PostgreSQL 在 ${TimeoutSeconds} 秒内未就绪: $Container"
    }
}

function Invoke-PostgresSql {
    param(
        [string]$Container,
        [string]$User,
        [string]$Database,
        [string]$Sql
    )

    $Sql | docker exec -i $Container psql -v ON_ERROR_STOP=1 -U $User -d $Database
    if ($LASTEXITCODE -ne 0) {
        throw "psql 执行失败: database=$Database"
    }
}

function Invoke-PostgresSqlFile {
    param(
        [string]$Container,
        [string]$User,
        [string]$Database,
        [string]$FilePath
    )

    Get-Content -LiteralPath $FilePath -Raw -Encoding UTF8 |
        docker exec -i $Container psql -v ON_ERROR_STOP=1 -U $User -d $Database -f -
    if ($LASTEXITCODE -ne 0) {
        throw "psql 文件导入失败: $FilePath"
    }
}

function Invoke-WebAlembicUpgrade {
    param(
        [string]$ComposeProject,
        [string]$BundleDir,
        [string]$ComposePath,
        [string]$ComposeEnvPath,
        [string]$Service
    )

    $dockerArgs = @("compose", "--project-name", $ComposeProject)
    if ((-not [string]::IsNullOrWhiteSpace($ComposeEnvPath)) -and (Test-Path -LiteralPath $ComposeEnvPath)) {
        $dockerArgs += @("--env-file", $ComposeEnvPath)
    }
    if (-not [string]::IsNullOrWhiteSpace($ComposePath)) {
        $dockerArgs += @("-f", $ComposePath)
    }
    $dockerArgs += @("run", "--rm", "--no-deps", $Service, "alembic", "upgrade", "head")

    if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
        Push-Location $BundleDir
    }
    try {
        docker @dockerArgs
        if ($LASTEXITCODE -ne 0) {
            throw "Alembic 迁移失败: project=$ComposeProject service=$Service"
        }
    }
    finally {
        if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
            Pop-Location
        }
    }
}

function Invoke-DockerComposeCommand {
    param(
        [string]$ComposeProject,
        [string]$BundleDir,
        [string]$ComposePath,
        [string]$ComposeEnvPath,
        [string[]]$ComposeArgs
    )

    $dockerArgs = @("compose", "--project-name", $ComposeProject)
    if ((-not [string]::IsNullOrWhiteSpace($ComposeEnvPath)) -and (Test-Path -LiteralPath $ComposeEnvPath)) {
        $dockerArgs += @("--env-file", $ComposeEnvPath)
    }
    if (-not [string]::IsNullOrWhiteSpace($ComposePath)) {
        $dockerArgs += @("-f", $ComposePath)
    }
    $dockerArgs += $ComposeArgs

    if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
        Push-Location $BundleDir
    }
    try {
        docker @dockerArgs
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose 执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
            Pop-Location
        }
    }
}

function Get-SeedSqlForDatabase {
    param(
        [string]$FilePath,
        [string]$TargetDatabase
    )

    $content = Get-Content -LiteralPath $FilePath -Raw -Encoding UTF8
    return ($content -replace "(?m)^\\connect\s+\S+\s*$", "\connect $TargetDatabase")
}

if (-not $Yes) {
    throw "该脚本会删除并重建数据库。请确认目标无误后追加 -Yes 执行。"
}

$seedSqlPath = Resolve-ScriptPath $SeedSql
if (-not [string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    $ReleaseRoot = (Resolve-Path $ReleaseRoot).Path
    if ([string]::IsNullOrWhiteSpace($ComposeFile)) {
        $ComposeFile = Join-Path $ReleaseRoot "docker-compose.yml"
    }
    if ([string]::IsNullOrWhiteSpace($EnvFile)) {
        $candidateEnvFile = Join-Path $ReleaseRoot "runtime\.env"
        if (Test-Path -LiteralPath $candidateEnvFile) {
            $EnvFile = $candidateEnvFile
        }
    }
}

$postgresContainer = Get-ContainerName -ComposeProject $ProjectName -BundleDir $ReleaseRoot -ComposePath $ComposeFile -ComposeEnvPath $EnvFile -Service $PostgresService

if ([string]::IsNullOrWhiteSpace($DatabaseName)) {
    $DatabaseName = Get-ContainerEnv -Container $postgresContainer -Name "POSTGRES_DB"
}
if ([string]::IsNullOrWhiteSpace($DatabaseUser)) {
    $DatabaseUser = Get-ContainerEnv -Container $postgresContainer -Name "POSTGRES_USER"
}
if ([string]::IsNullOrWhiteSpace($DatabasePassword)) {
    $DatabasePassword = Get-ContainerEnv -Container $postgresContainer -Name "POSTGRES_PASSWORD"
}
if ([string]::IsNullOrWhiteSpace($DatabaseName)) {
    throw "无法确定数据库名，请传入 -DatabaseName。"
}
if ([string]::IsNullOrWhiteSpace($DatabaseUser)) {
    throw "无法确定数据库用户，请传入 -DatabaseUser。"
}

Write-Host "目标 Compose 项目: $ProjectName"
if (-not [string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    Write-Host "Release 目录: $ReleaseRoot"
}
Write-Host "PostgreSQL 容器: $postgresContainer"
Write-Host "目标数据库: $DatabaseName"
Write-Host "目标用户: $DatabaseUser"

Wait-PostgresReady -Container $postgresContainer -User $DatabaseUser -Database $DatabaseName -TimeoutSeconds $WaitSeconds

Write-Host "停止 web 服务，避免重置数据库时触发容器重启竞争"
Invoke-DockerComposeCommand -ComposeProject $ProjectName -BundleDir $ReleaseRoot -ComposePath $ComposeFile -ComposeEnvPath $EnvFile -ComposeArgs @("stop", $WebService)

$escapedDatabase = $DatabaseName.Replace("'", "''")
$escapedUser = $DatabaseUser.Replace("'", "''")
$escapedPassword = $DatabasePassword.Replace("'", "''")

Write-Host "断开连接并删除数据库: $DatabaseName"
$dropSql = @"
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE datname = '$escapedDatabase'
  AND pid <> pg_backend_pid();
DROP DATABASE IF EXISTS "$DatabaseName";
CREATE DATABASE "$DatabaseName" OWNER "$DatabaseUser" TEMPLATE template0 ENCODING 'UTF8';
ALTER USER "$DatabaseUser" WITH PASSWORD '$escapedPassword';
"@
Invoke-PostgresSql -Container $postgresContainer -User $DatabaseUser -Database "postgres" -Sql $dropSql

Write-Host "执行 Alembic 迁移到最新结构"
Invoke-WebAlembicUpgrade -ComposeProject $ProjectName -BundleDir $ReleaseRoot -ComposePath $ComposeFile -ComposeEnvPath $EnvFile -Service $WebService

Write-Host "导入测试数据 SQL: $seedSqlPath"
$seedSql = Get-SeedSqlForDatabase -FilePath $seedSqlPath -TargetDatabase $DatabaseName
$seedSql | docker exec -i $postgresContainer psql -v ON_ERROR_STOP=1 -U $DatabaseUser -d $DatabaseName -f -
if ($LASTEXITCODE -ne 0) {
    throw "测试数据导入失败: $seedSqlPath"
}

Write-Host "重新启动 web 服务"
Invoke-DockerComposeCommand -ComposeProject $ProjectName -BundleDir $ReleaseRoot -ComposePath $ComposeFile -ComposeEnvPath $EnvFile -ComposeArgs @("up", "-d", "--no-build", $WebService)

Write-Host "完成：数据库已重置、迁移已执行、测试数据已导入。"


