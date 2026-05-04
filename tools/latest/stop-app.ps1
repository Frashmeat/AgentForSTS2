<#
.SYNOPSIS
停止 AgentTheSpire app 主线部署。

.DESCRIPTION
读取 runtime/app-deploy-state.json 停止本机 frontend 与 local-workstation，并对 Docker 平台栈执行 compose down。

.PARAMETER ReleaseRoot
release 目录。留空时识别当前仓库。

.PARAMETER ConfigPath
唯一人工配置文件。默认 runtime/agentthespire.config.json。

.PARAMETER Help
显示帮助。

.EXAMPLE
pwsh -File .\tools\latest\stop-app.ps1
#>
[CmdletBinding()]
param(
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Alias("c")]
    [string]$ConfigPath = "",

    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
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

function Resolve-AppLayout {
    param([string]$PreferredReleaseRoot)
    $repoRoot = Get-RepoRoot
    if ([string]::IsNullOrWhiteSpace($PreferredReleaseRoot)) {
        if (Test-Path -LiteralPath (Join-Path $repoRoot "services\local-workstation\backend\main_workstation.py")) {
            return @{
                Mode = "release"
                Root = $repoRoot
                ConfigRoot = Join-Path $repoRoot "runtime"
                ComposeFile = Join-Path $repoRoot "docker-compose.yml"
            }
        }
        if (Test-Path -LiteralPath (Join-Path $repoRoot "backend\main_workstation.py")) {
            return @{
                Mode = "repo"
                Root = $repoRoot
                ConfigRoot = Join-Path $repoRoot "runtime"
                ComposeFile = Join-Path $repoRoot "tools\latest\templates\compose.app.yml"
            }
        }
        $PreferredReleaseRoot = Join-Path $PSScriptRoot "artifacts\agentthespire-app-release"
    }
    $releaseRoot = (Resolve-Path $PreferredReleaseRoot).Path
    return @{
        Mode = "release"
        Root = $releaseRoot
        ConfigRoot = Join-Path $releaseRoot "runtime"
        ComposeFile = Join-Path $releaseRoot "docker-compose.yml"
    }
}

function Get-ConfigPath {
    param([hashtable]$Layout, [string]$PreferredPath)
    if (-not [string]::IsNullOrWhiteSpace($PreferredPath)) {
        return $PreferredPath
    }
    return Join-Path $Layout.ConfigRoot "agentthespire.config.json"
}

function Get-ProjectName {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return "agentthespire-app"
    }
    $config = Read-JsonHashtableFile -Path $Path
    if ($config.ContainsKey("docker") -and $config.docker.ContainsKey("project_name")) {
        $value = [string]$config.docker.project_name
        if (-not [string]::IsNullOrWhiteSpace($value)) {
            return $value
        }
    }
    return "agentthespire-app"
}

$layout = Resolve-AppLayout -PreferredReleaseRoot $ReleaseRoot
$configPathResolved = Get-ConfigPath -Layout $layout -PreferredPath $ConfigPath
$statePath = Join-Path $layout.ConfigRoot "app-deploy-state.json"

if (Test-Path -LiteralPath $statePath) {
    $state = Get-Content -LiteralPath $statePath -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($entry in @($state.processes)) {
        if ($null -eq $entry -or $null -eq $entry.pid) {
            continue
        }
        try {
            Stop-Process -Id ([int]$entry.pid) -Force -ErrorAction Stop
        } catch {
        }
    }
    Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
    Write-Host "已停止本机 frontend / local-workstation 进程。"
} else {
    Write-Host "未找到本机部署状态文件，无需停止本机进程：$statePath"
}

$envFile = Join-Path (Join-Path $layout.ConfigRoot "generated") "docker.env"
if ((Test-Path -LiteralPath $layout.ComposeFile) -and (Test-Path -LiteralPath $envFile) -and (Get-Command docker -ErrorAction SilentlyContinue)) {
    $projectName = Get-ProjectName -Path $configPathResolved
    Push-Location $layout.Root
    try {
        docker compose --project-name $projectName --env-file $envFile -f $layout.ComposeFile down --remove-orphans
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose down 失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
    Write-Host "已停止 Docker web / web-workstation / postgres。"
} else {
    Write-Host "未执行 Docker down：缺少 compose/env/docker 之一。"
}
