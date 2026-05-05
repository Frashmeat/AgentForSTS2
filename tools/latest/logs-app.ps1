<#
.SYNOPSIS
Show AgentTheSpire app deployment logs.

.DESCRIPTION
Read runtime/agentthespire.config.json and runtime/generated/docker.env, then show:
- local frontend / local-workstation log files
- Docker postgres / web-workstation / web logs

.PARAMETER ReleaseRoot
Release directory. Empty means current repository/release auto-detection.

.PARAMETER ConfigPath
Single human-edited config file. Default: runtime/agentthespire.config.json.

.PARAMETER Service
Log service. Default: all. One of all / frontend / local-workstation / postgres / web-workstation / web.

.PARAMETER Tail
Number of latest lines to show. Default: 200.

.PARAMETER Follow
Follow logs. For one local service, follows local log files. For Docker service/all, follows docker compose logs.

.PARAMETER LocalOnly
Only show local frontend / local-workstation logs.

.PARAMETER DockerOnly
Only show Docker postgres / web-workstation / web logs.

.PARAMETER Help
Show help.

.EXAMPLE
pwsh -File .\tools\latest\logs-app.ps1

.EXAMPLE
pwsh -File .\tools\latest\logs-app.ps1 -Service web -Follow
#>
[CmdletBinding()]
param(
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Alias("c")]
    [string]$ConfigPath = "",

    [ValidateSet("all", "frontend", "local-workstation", "postgres", "web-workstation", "web")]
    [string]$Service = "all",

    [int]$Tail = 200,

    [switch]$Follow,

    [switch]$LocalOnly,

    [switch]$DockerOnly,

    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

if ($LocalOnly.IsPresent -and $DockerOnly.IsPresent) {
    throw "-LocalOnly and -DockerOnly cannot be used together."
}

if ($Tail -lt 1) {
    throw "-Tail must be greater than 0."
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

function Convert-PathForComposeEnv {
    param([string]$Path)
    return ([System.IO.Path]::GetFullPath($Path) -replace "\\", "/")
}

function Ensure-DockerEnvPlatformRuntimeDir {
    param([hashtable]$Layout, [string]$EnvFile)
    if (-not (Test-Path -LiteralPath $EnvFile)) {
        return
    }
    $content = Get-Content -LiteralPath $EnvFile -Raw -Encoding UTF8
    if ($content -match "(?m)^ATS_PLATFORM_RUNTIME_DIR=") {
        return
    }
    $platformRuntimeDir = Join-Path $Layout.ConfigRoot "platform"
    New-Item -ItemType Directory -Path $platformRuntimeDir -Force | Out-Null
    Add-Content -LiteralPath $EnvFile -Encoding UTF8 -Value (
        "ATS_PLATFORM_RUNTIME_DIR=$(Convert-PathForComposeEnv -Path $platformRuntimeDir)"
    )
}

function Get-LocalLogFiles {
    param([hashtable]$Layout, [string]$SelectedService)
    $logRoot = Join-Path $Layout.ConfigRoot "logs"
    $services = if ($SelectedService -eq "all") { @("frontend", "local-workstation") } else { @($SelectedService) }
    $files = @()
    foreach ($serviceName in $services) {
        if ($serviceName -notin @("frontend", "local-workstation")) {
            continue
        }
        $files += @(
            Join-Path $logRoot ("{0}.stdout.log" -f $serviceName)
            Join-Path $logRoot ("{0}.stderr.log" -f $serviceName)
        )
    }
    return $files
}

function Show-LocalLogs {
    param([string[]]$Files, [int]$LineCount, [switch]$Wait)
    if ($Files.Count -eq 0) {
        return
    }

    Write-Host ""
    Write-Host "Local log files:" -ForegroundColor Cyan
    foreach ($file in $Files) {
        Write-Host "  $file"
    }

    if ($Wait -and $Files.Count -gt 2) {
        Write-Host ""
        Write-Host "Tip: specify one local service when following local logs, for example: logs app -Service local-workstation -Follow." -ForegroundColor Yellow
        return
    }

    foreach ($file in $Files) {
        Write-Host ""
        Write-Host ("===== {0} =====" -f $file) -ForegroundColor Cyan
        if (-not (Test-Path -LiteralPath $file)) {
            Write-Host "Log file does not exist. The service may not have started yet."
            continue
        }
        if ($Wait) {
            Get-Content -LiteralPath $file -Tail $LineCount -Wait
        } else {
            Get-Content -LiteralPath $file -Tail $LineCount
        }
    }
}

function Invoke-DockerLogs {
    param(
        [hashtable]$Layout,
        [string]$EnvFile,
        [string]$ProjectName,
        [string]$SelectedService,
        [int]$LineCount,
        [switch]$Wait
    )
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        Write-Host "docker command not found; skipping Docker logs."
        return
    }
    if (-not (Test-Path -LiteralPath $Layout.ComposeFile)) {
        Write-Host "Compose file does not exist; skipping Docker logs: $($Layout.ComposeFile)"
        return
    }
    if (-not (Test-Path -LiteralPath $EnvFile)) {
        Write-Host "Docker env file does not exist; skipping Docker logs: $EnvFile"
        return
    }
    Ensure-DockerEnvPlatformRuntimeDir -Layout $Layout -EnvFile $EnvFile

    $services = if ($SelectedService -eq "all") { @("postgres", "web-workstation", "web") } else { @($SelectedService) }
    $services = @($services | Where-Object { $_ -in @("postgres", "web-workstation", "web") })
    if ($services.Count -eq 0) {
        return
    }

    $dockerArgs = @(
        "compose",
        "--project-name", $ProjectName,
        "--env-file", $EnvFile,
        "-f", $Layout.ComposeFile,
        "logs",
        "--tail", [string]$LineCount
    )
    if ($Wait) {
        $dockerArgs += "-f"
    }
    $dockerArgs += $services

    Write-Host ""
    Write-Host ("Docker logs: docker {0}" -f ($dockerArgs -join " ")) -ForegroundColor Cyan
    Push-Location $Layout.Root
    try {
        & docker @dockerArgs
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose logs failed, exit code: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

$layout = Resolve-AppLayout -PreferredReleaseRoot $ReleaseRoot
$configPathResolved = Get-ConfigPath -Layout $layout -PreferredPath $ConfigPath
$envFile = Join-Path (Join-Path $layout.ConfigRoot "generated") "docker.env"
$projectName = Get-ProjectName -Path $configPathResolved

$localServiceSelected = $Service -in @("all", "frontend", "local-workstation")
$dockerServiceSelected = $Service -in @("all", "postgres", "web-workstation", "web")

if (-not $DockerOnly -and $localServiceSelected) {
    Show-LocalLogs -Files (Get-LocalLogFiles -Layout $layout -SelectedService $Service) -LineCount $Tail -Wait:($Follow.IsPresent)
}

if (-not $LocalOnly -and $dockerServiceSelected) {
    Invoke-DockerLogs -Layout $layout -EnvFile $envFile -ProjectName $projectName -SelectedService $Service -LineCount $Tail -Wait:($Follow.IsPresent)
}
