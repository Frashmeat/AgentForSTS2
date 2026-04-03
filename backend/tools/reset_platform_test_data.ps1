param(
    [string]$ContainerName = "agentthespire-postgres-internal",
    [string]$PostgresUser = "postgres",
    [string]$AdminDatabase = "postgres",
    [string]$SchemaSql = "..\migrations\sql\2026-03-31-create-platform-database.sql",
    [string]$SeedSql = "..\migrations\sql\2026-03-31-seed-platform-test-data.sql",
    [int]$WaitSeconds = 30
)

$ErrorActionPreference = "Stop"

function Resolve-ScriptPath {
    param([string]$RelativePath)

    return (Resolve-Path (Join-Path $PSScriptRoot $RelativePath)).Path
}

function Invoke-DockerExecPsqlFile {
    param(
        [string]$Container,
        [string]$User,
        [string]$Database,
        [string]$FilePath
    )

    Get-Content -LiteralPath $FilePath -Raw |
        docker exec -i $Container psql -v ON_ERROR_STOP=1 -U $User -d $Database -f -
}

$schemaSqlPath = Resolve-ScriptPath $SchemaSql
$seedSqlPath = Resolve-ScriptPath $SeedSql

Write-Host "重启 PostgreSQL 容器: $ContainerName"
docker restart $ContainerName | Out-Null

Write-Host "等待 PostgreSQL 就绪"
$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
    docker exec $ContainerName pg_isready -U $PostgresUser | Out-Null
    if ($LASTEXITCODE -eq 0) {
        break
    }
    Start-Sleep -Seconds 1
}

docker exec $ContainerName pg_isready -U $PostgresUser | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "PostgreSQL 容器在 ${WaitSeconds} 秒内未就绪: $ContainerName"
}

Write-Host "导入建库建表 SQL: $schemaSqlPath"
Invoke-DockerExecPsqlFile -Container $ContainerName -User $PostgresUser -Database $AdminDatabase -FilePath $schemaSqlPath

Write-Host "导入测试数据 SQL: $seedSqlPath"
Invoke-DockerExecPsqlFile -Container $ContainerName -User $PostgresUser -Database $AdminDatabase -FilePath $seedSqlPath

Write-Host "完成：已重启容器并导入平台测试数据"
