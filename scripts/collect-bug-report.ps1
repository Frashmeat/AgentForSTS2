# Collect a redacted diagnostic bundle for AgentTheSpire smoke-test bugs.
#
# Usage:
#   .\scripts\collect-bug-report.ps1 -ProjectRoot <path> [-OutDir <dir>]
#
# Produces: <OutDir>\bug-report-YYYYMMDD-HHMMSS.zip
#
# Captures (all secrets redacted to [REDACTED]):
#   - runtime\agentthespire.config.json
#   - %APPDATA%\AgentTheSpire\{config.json,recent_projects.json,logs\*}
#   - <ProjectRoot>\.ats\{audit.log,version,lock}
#   - <ProjectRoot>\history\*.json  (newest 50 by LastWriteTime)
#   - <ProjectRoot>\items\*.status.json
#   - sysinfo.txt (OS / dotnet / ilspycmd / git HEAD / cargo)
#
# Compatible with Windows PowerShell 5.1.

[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [string]$ProjectRoot,

    [string]$OutDir = ".\bug-reports",

    [int]$HistoryLimit = 50
)

$ErrorActionPreference = "Stop"

# Resolve absolute paths early; Push-Location/Resolve-Path can fail later if cwd changes.
$ProjectRoot = (Resolve-Path -LiteralPath $ProjectRoot).Path
if (-not (Test-Path -LiteralPath $ProjectRoot -PathType Container)) {
    throw "ProjectRoot is not a directory: $ProjectRoot"
}

if (-not (Test-Path -LiteralPath $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
}
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path

$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$staging = Join-Path $env:TEMP "ats-bug-report-$timestamp"
New-Item -ItemType Directory -Path $staging -Force | Out-Null

Write-Host "Staging:  $staging"
Write-Host "Project:  $ProjectRoot"
Write-Host "Repo:     $RepoRoot"

# Keys that must NEVER leave the user machine in plaintext.
$SensitiveKeys = @(
    "api_key", "apiKey",
    "session_secret", "sessionSecret",
    "credential_secret", "credentialSecret",
    "password", "secret", "token"
)

function Redact-JsonFile {
    param(
        [string]$SrcPath,
        [string]$DstPath
    )
    if (-not (Test-Path -LiteralPath $SrcPath)) { return $false }
    try {
        $raw = Get-Content -LiteralPath $SrcPath -Raw -Encoding UTF8
        $obj = $raw | ConvertFrom-Json -ErrorAction Stop
        Redact-Object -Node $obj
        $out = $obj | ConvertTo-Json -Depth 32
        $dstDir = Split-Path -Parent $DstPath
        if (-not (Test-Path -LiteralPath $dstDir)) {
            New-Item -ItemType Directory -Path $dstDir -Force | Out-Null
        }
        $out | Out-File -LiteralPath $DstPath -Encoding utf8
        return $true
    } catch {
        # JSON parse failed - copy raw but warn. Don't block whole capture.
        Write-Warning ("Could not parse JSON, copying raw: {0} ({1})" -f $SrcPath, $_.Exception.Message)
        Copy-Item -LiteralPath $SrcPath -Destination $DstPath -Force
        return $true
    }
}

function Redact-Object {
    param($Node)
    if ($null -eq $Node) { return }
    if ($Node -is [System.Collections.IDictionary]) {
        foreach ($key in @($Node.Keys)) {
            if ($SensitiveKeys -contains $key) {
                if (-not [string]::IsNullOrEmpty([string]$Node[$key])) {
                    $Node[$key] = "[REDACTED]"
                }
            } else {
                Redact-Object -Node $Node[$key]
            }
        }
        return
    }
    if ($Node -is [PSCustomObject]) {
        foreach ($prop in $Node.PSObject.Properties) {
            if ($SensitiveKeys -contains $prop.Name) {
                if (-not [string]::IsNullOrEmpty([string]$prop.Value)) {
                    $prop.Value = "[REDACTED]"
                }
            } else {
                Redact-Object -Node $prop.Value
            }
        }
        return
    }
    if ($Node -is [System.Collections.IEnumerable] -and -not ($Node -is [string])) {
        foreach ($item in $Node) {
            Redact-Object -Node $item
        }
        return
    }
}

# ---- 1. runtime config ----
$runtimeConfig = Join-Path $RepoRoot "runtime\agentthespire.config.json"
if (Test-Path -LiteralPath $runtimeConfig) {
    [void](Redact-JsonFile -SrcPath $runtimeConfig -DstPath (Join-Path $staging "runtime-config.json"))
    Write-Host "  + runtime config (redacted)"
} else {
    "runtime/agentthespire.config.json not found" | Out-File -LiteralPath (Join-Path $staging "runtime-config.MISSING.txt") -Encoding utf8
    Write-Host "  - runtime config not found"
}

# ---- 2. AppData ----
$appData = Join-Path $env:APPDATA "AgentTheSpire"
$appDataDst = Join-Path $staging "appdata"
New-Item -ItemType Directory -Path $appDataDst -Force | Out-Null
if (Test-Path -LiteralPath $appData) {
    foreach ($f in @("config.json", "recent_projects.json")) {
        $src = Join-Path $appData $f
        if (Test-Path -LiteralPath $src) {
            [void](Redact-JsonFile -SrcPath $src -DstPath (Join-Path $appDataDst $f))
            Write-Host "  + appdata\$f"
        }
    }
    $logsSrc = Join-Path $appData "logs"
    if (Test-Path -LiteralPath $logsSrc) {
        $logsDst = Join-Path $appDataDst "logs"
        New-Item -ItemType Directory -Path $logsDst -Force | Out-Null
        Get-ChildItem -LiteralPath $logsSrc -File -ErrorAction SilentlyContinue | ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $logsDst $_.Name) -Force
        }
        Write-Host "  + appdata\logs\"
    }
} else {
    "AppData root not found: $appData" | Out-File -LiteralPath (Join-Path $appDataDst "MISSING.txt") -Encoding utf8
}

# ---- 3. Project .ats/ ----
$projAts = Join-Path $ProjectRoot ".ats"
$projDst = Join-Path $staging "project"
New-Item -ItemType Directory -Path $projDst -Force | Out-Null
if (Test-Path -LiteralPath $projAts) {
    $atsDst = Join-Path $projDst ".ats"
    New-Item -ItemType Directory -Path $atsDst -Force | Out-Null
    foreach ($f in @("audit.log", "version", "lock")) {
        $src = Join-Path $projAts $f
        if (Test-Path -LiteralPath $src) {
            Copy-Item -LiteralPath $src -Destination (Join-Path $atsDst $f) -Force
            Write-Host "  + project\.ats\$f"
        }
    }
} else {
    Write-Warning "Project .ats not found: $projAts"
}

# project.json (mostly safe but no harm redacting)
$projectJson = Join-Path $ProjectRoot "project.json"
if (Test-Path -LiteralPath $projectJson) {
    [void](Redact-JsonFile -SrcPath $projectJson -DstPath (Join-Path $projDst "project.json"))
    Write-Host "  + project\project.json"
}

# plan.json
$planJson = Join-Path $ProjectRoot "plan.json"
if (Test-Path -LiteralPath $planJson) {
    Copy-Item -LiteralPath $planJson -Destination (Join-Path $projDst "plan.json") -Force
    Write-Host "  + project\plan.json"
}

# ---- 4. Project history (newest N) ----
$historySrc = Join-Path $ProjectRoot "history"
if (Test-Path -LiteralPath $historySrc) {
    $historyDst = Join-Path $projDst "history"
    New-Item -ItemType Directory -Path $historyDst -Force | Out-Null
    $files = Get-ChildItem -LiteralPath $historySrc -File -Filter "*.json" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First $HistoryLimit
    foreach ($f in $files) {
        [void](Redact-JsonFile -SrcPath $f.FullName -DstPath (Join-Path $historyDst $f.Name))
    }
    Write-Host ("  + project\history\ ({0} files, newest {1})" -f $files.Count, $HistoryLimit)
}

# ---- 5. Project items/*.status.json ----
$itemsSrc = Join-Path $ProjectRoot "items"
if (Test-Path -LiteralPath $itemsSrc) {
    $itemsDst = Join-Path $projDst "items"
    New-Item -ItemType Directory -Path $itemsDst -Force | Out-Null
    Get-ChildItem -LiteralPath $itemsSrc -File -Filter "*.status.json" -ErrorAction SilentlyContinue | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $itemsDst $_.Name) -Force
    }
    Write-Host "  + project\items\*.status.json"
}

# ---- 6. sysinfo.txt ----
function Try-Run {
    param([string]$Label, [scriptblock]$Block)
    "==== $Label ===="
    try {
        & $Block
    } catch {
        "ERROR: $($_.Exception.Message)"
    }
    ""
}

$sysinfoLines = @()
$sysinfoLines += "Captured: $(Get-Date -Format 'o')"
$sysinfoLines += "Host: $env:COMPUTERNAME"
$sysinfoLines += "User: $env:USERNAME"
$sysinfoLines += ""
$sysinfoLines += Try-Run "OS" { (Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture | Format-List | Out-String).Trim() }
$sysinfoLines += Try-Run "PowerShell" { $PSVersionTable | Format-List | Out-String }
$sysinfoLines += Try-Run "dotnet --info" {
    $cmd = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($cmd) { dotnet --info } else { "dotnet not in PATH" }
}
$sysinfoLines += Try-Run "ilspycmd --version" {
    $cmd = Get-Command ilspycmd -ErrorAction SilentlyContinue
    if ($cmd) { ilspycmd --version } else { "ilspycmd not in PATH (try: dotnet tool install -g ilspycmd)" }
}
$sysinfoLines += Try-Run "git rev-parse HEAD" {
    Push-Location $RepoRoot
    try {
        $branch = git rev-parse --abbrev-ref HEAD
        $head = git rev-parse HEAD
        $status = git status --short
        "branch: $branch"
        "head:   $head"
        "status:"
        if ($status) { $status } else { "  (clean)" }
    } finally { Pop-Location }
}
$sysinfoLines += Try-Run "cargo --version" {
    $cmd = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cmd) { cargo --version } else { "cargo not in PATH" }
}
$sysinfoLines += Try-Run "node --version / npm --version" {
    $node = Get-Command node -ErrorAction SilentlyContinue
    $npm = Get-Command npm -ErrorAction SilentlyContinue
    if ($node) { "node: $(node --version)" } else { "node not in PATH" }
    if ($npm)  { "npm:  $(npm --version)" }  else { "npm not in PATH" }
}

($sysinfoLines -join "`r`n") | Out-File -LiteralPath (Join-Path $staging "sysinfo.txt") -Encoding utf8
Write-Host "  + sysinfo.txt"

# ---- 7. README inside zip ----
$readme = @"
AgentTheSpire bug report bundle
================================
Generated: $(Get-Date -Format 'o')
Project:   $ProjectRoot
Repo HEAD: see sysinfo.txt

Contents:
  runtime-config.json    Redacted runtime config (api_key, secrets -> [REDACTED])
  appdata/               Redacted %APPDATA%\AgentTheSpire\ contents
  project/.ats/          audit.log + version + lock
  project/project.json   Project metadata (redacted)
  project/plan.json      Current ModPlan if present
  project/history/       Newest $HistoryLimit job records (redacted)
  project/items/         PlanItem status.json files
  sysinfo.txt            OS / dotnet / ilspycmd / git HEAD / cargo / node

Secrets removed (case-sensitive key match):
  api_key, apiKey, session_secret, sessionSecret,
  credential_secret, credentialSecret, password, secret, token

If a non-JSON file contained a secret, it was NOT redacted (raw copy).
Inspect manually before posting to a public issue tracker.
"@
$readme | Out-File -LiteralPath (Join-Path $staging "README.txt") -Encoding utf8

# ---- 8. Zip ----
$zipPath = Join-Path $OutDir "bug-report-$timestamp.zip"
if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory($staging, $zipPath)

Write-Host ""
Write-Host "Done: $zipPath"
Write-Host "Size: $((Get-Item -LiteralPath $zipPath).Length) bytes"
Write-Host ""
Write-Host "Inspect with:"
Write-Host "  Expand-Archive -LiteralPath `"$zipPath`" -DestinationPath .\inspect"
Write-Host ""
Write-Host "Attach this zip to a new GitHub issue using the smoke-bug template."

# Cleanup staging
Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
