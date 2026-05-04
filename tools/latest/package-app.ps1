<#
.SYNOPSIS
打包 AgentTheSpire 唯一主线 app release。

.DESCRIPTION
生成固定拓扑 release：本机 frontend + local-workstation，Docker web + web-workstation + postgres。
直接执行脚本会按默认参数打包；前端构建属于较重步骤，可用 -SkipFrontendBuild 跳过。

.PARAMETER OutputRoot
输出目录。默认 tools/latest/artifacts。

.PARAMETER ReleaseName
发布目录名。默认 agentthespire-app-release。

.PARAMETER SkipFrontendBuild
跳过前端构建。仅在已确认 frontend/dist 最新时使用。

.PARAMETER SkipZip
跳过 zip 归档。

.PARAMETER Help
显示帮助。

.EXAMPLE
pwsh -File .\tools\latest\package-app.ps1

.EXAMPLE
pwsh -File .\tools\latest\package-app.ps1 -NoZip
#>
[CmdletBinding()]
param(
    [Alias("o")]
    [string]$OutputRoot = "",

    [Alias("n")]
    [string]$ReleaseName = "agentthespire-app-release",

    [Alias("NoFrontend")]
    [switch]$SkipFrontendBuild,

    [Alias("NoZip")]
    [switch]$SkipZip,

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

function Remove-DirectoryIfExists {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

function Remove-FileIfExists {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        Remove-Item -LiteralPath $Path -Force
    }
}

function New-CleanDirectory {
    param(
        [string]$Path,
        [string[]]$PreserveRelativePaths = @()
    )

    $null = New-Item -ItemType Directory -Path $Path -Force
    if ($PreserveRelativePaths.Count -eq 0) {
        foreach ($child in Get-ChildItem -LiteralPath $Path -Force) {
            Remove-Item -LiteralPath $child.FullName -Recurse -Force
        }
        return
    }

    foreach ($child in Get-ChildItem -LiteralPath $Path -Force) {
        $relativePath = [System.IO.Path]::GetRelativePath($Path, $child.FullName)
        $preserve = $false
        foreach ($preserved in $PreserveRelativePaths) {
            if (
                $relativePath -eq $preserved -or
                $relativePath.StartsWith("$preserved\") -or
                $preserved.StartsWith("$relativePath\")
            ) {
                $preserve = $true
                break
            }
        }
        if (-not $preserve) {
            try {
                Remove-Item -LiteralPath $child.FullName -Recurse -Force
            } catch {
                Write-Warning ("清理 release 目录失败：{0}" -f $child.FullName)
                Write-Warning "如该目录被本机服务、日志窗口或当前工作目录占用，请先执行：powershell -File .\tools\tools.ps1 stop app"
                throw
            }
        }
    }
}

function Invoke-RobocopySafe {
    param(
        [string]$Source,
        [string]$Destination,
        [string[]]$ExcludeDirectories = @(),
        [string[]]$ExcludeFiles = @()
    )

    $null = New-Item -ItemType Directory -Path $Destination -Force
    $arguments = @($Source, $Destination, "/E", "/NFL", "/NDL", "/NJH", "/NJS", "/NP")
    if ($ExcludeDirectories.Count -gt 0) {
        $arguments += "/XD"
        $arguments += $ExcludeDirectories
    }
    if ($ExcludeFiles.Count -gt 0) {
        $arguments += "/XF"
        $arguments += $ExcludeFiles
    }

    & robocopy @arguments | Out-Null
    if ($LASTEXITCODE -gt 7) {
        throw "robocopy 失败，退出码: $LASTEXITCODE"
    }
}

function Resolve-NpmCommand {
    $npmCmdPath = Join-Path ${env:ProgramFiles} "nodejs\npm.cmd"
    if (Test-Path -LiteralPath $npmCmdPath) {
        return $npmCmdPath
    }
    return "npm"
}

function Ensure-FrontendBuild {
    param([string]$FrontendDir)

    if ($SkipFrontendBuild) {
        return
    }

    Assert-CommandExists -CommandName "node"
    Assert-CommandExists -CommandName "npm"
    $npmCommand = Resolve-NpmCommand
    Push-Location $FrontendDir
    try {
        if (-not (Test-Path -LiteralPath (Join-Path $FrontendDir "node_modules"))) {
            & $npmCommand install
            if ($LASTEXITCODE -ne 0) {
                throw "前端依赖安装失败"
            }
        }
        & $npmCommand run build
        if ($LASTEXITCODE -ne 0) {
            throw "前端构建失败"
        }
    }
    finally {
        Pop-Location
    }
}

function Initialize-KnowledgeRuntimeDirs {
    param([string]$ReleaseDir)

    foreach ($relativePath in @(
        "runtime\knowledge\game",
        "runtime\knowledge\baselib",
        "runtime\knowledge\resources\sts2",
        "runtime\knowledge\cache",
        "runtime\knowledge\packs",
        "runtime\generated",
        "runtime\logs",
        "runtime\web",
        "runtime\web-workstation"
    )) {
        $null = New-Item -ItemType Directory -Path (Join-Path $ReleaseDir $relativePath) -Force
    }
}

function Copy-RuntimeToolsBundle {
    param([string]$ReleaseDir, [string]$RepoRoot)

    $sourceToolsDir = Join-Path (Join-Path $RepoRoot "runtime") "tools"
    if (-not (Test-Path -LiteralPath $sourceToolsDir)) {
        return
    }
    $runtimeDir = Join-Path $ReleaseDir "runtime"
    $targetToolsDir = Join-Path $runtimeDir "tools"
    Remove-DirectoryIfExists -Path $targetToolsDir
    Copy-Item -LiteralPath $sourceToolsDir -Destination $runtimeDir -Recurse -Force
}

function Write-ReleaseManifest {
    param([string]$ReleaseDir, [string]$RepoRoot)

    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
    $manifest = [ordered]@{
        release_name = Split-Path -Leaf $ReleaseDir
        target = "app"
        created_at = (Get-Date).ToString("s")
        git_commit = $commit
        topology = "local frontend + local-workstation; docker web + web-workstation + postgres"
        config_source = "runtime/agentthespire.config.json"
        compose_file = "docker-compose.yml"
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $ReleaseDir "release-manifest.json") -Encoding UTF8
}

function New-ZipFromDirectory {
    param([string]$SourceDir, [string]$DestinationPath)

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    Remove-FileIfExists -Path $DestinationPath
    [System.IO.Compression.ZipFile]::CreateFromDirectory($SourceDir, $DestinationPath)
}

$repoRoot = Get-RepoRoot
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $PSScriptRoot "artifacts"
}

$releaseDir = Join-Path $OutputRoot $ReleaseName
$zipPath = Join-Path $OutputRoot ("{0}.zip" -f $ReleaseName)
$frontendDir = Join-Path $repoRoot "frontend"
$frontendDist = Join-Path $frontendDir "dist"
$backendDir = Join-Path $repoRoot "backend"
$modTemplateDir = Join-Path $repoRoot "mod_template"
$templateRoot = Join-Path $PSScriptRoot "templates"

Assert-CommandExists -CommandName "git"
Assert-CommandExists -CommandName "robocopy"
Assert-PathExists -Path $backendDir -Label "backend 目录"
Assert-PathExists -Path $frontendDir -Label "frontend 目录"
Assert-PathExists -Path $modTemplateDir -Label "mod_template 目录"
Assert-PathExists -Path (Join-Path $templateRoot "compose.app.yml") -Label "app compose 模板"

Ensure-FrontendBuild -FrontendDir $frontendDir
Assert-PathExists -Path $frontendDist -Label "frontend/dist 构建产物"

$null = New-Item -ItemType Directory -Path $OutputRoot -Force
New-CleanDirectory -Path $releaseDir -PreserveRelativePaths @(
    "runtime\agentthespire.config.json",
    "runtime\knowledge",
    "runtime\logs",
    "runtime\python-runtime",
    "runtime\.env",
    "runtime\generated\docker.env"
)

foreach ($relativePath in @(
    "services\local-workstation",
    "services\frontend\frontend",
    "services\web",
    "services\web-workstation",
    "tools\latest",
    "runtime"
)) {
    $null = New-Item -ItemType Directory -Path (Join-Path $releaseDir $relativePath) -Force
}

Write-Host "整理 local-workstation..."
Invoke-RobocopySafe -Source $backendDir -Destination (Join-Path $releaseDir "services\local-workstation\backend") -ExcludeDirectories @(".venv", ".tmp", "tests", "__pycache__", ".pytest_cache") -ExcludeFiles @("*.pyc")
Invoke-RobocopySafe -Source $frontendDist -Destination (Join-Path $releaseDir "services\local-workstation\frontend\dist")
Invoke-RobocopySafe -Source $modTemplateDir -Destination (Join-Path $releaseDir "services\local-workstation\mod_template") -ExcludeDirectories @(".godot", ".mono", "bin", "obj")
Copy-Item -LiteralPath (Join-Path $repoRoot "config.example.json") -Destination (Join-Path $releaseDir "services\local-workstation\config.example.json") -Force

Write-Host "整理 frontend..."
Invoke-RobocopySafe -Source $frontendDist -Destination (Join-Path $releaseDir "services\frontend\frontend\dist")

Write-Host "整理 web..."
Invoke-RobocopySafe -Source $backendDir -Destination (Join-Path $releaseDir "services\web\backend") -ExcludeDirectories @(".venv", ".tmp", "tests", "__pycache__", ".pytest_cache") -ExcludeFiles @("*.pyc")
Copy-Item -LiteralPath (Join-Path $repoRoot "config.example.json") -Destination (Join-Path $releaseDir "services\web\config.example.json") -Force
Copy-Item -LiteralPath (Join-Path $templateRoot "web\Dockerfile") -Destination (Join-Path $releaseDir "services\web\Dockerfile") -Force
Copy-Item -LiteralPath (Join-Path $templateRoot "web\.dockerignore") -Destination (Join-Path $releaseDir "services\web\.dockerignore") -Force

Write-Host "整理 web-workstation..."
Invoke-RobocopySafe -Source $backendDir -Destination (Join-Path $releaseDir "services\web-workstation\backend") -ExcludeDirectories @(".venv", ".tmp", "tests", "__pycache__", ".pytest_cache") -ExcludeFiles @("*.pyc")
Invoke-RobocopySafe -Source $frontendDist -Destination (Join-Path $releaseDir "services\web-workstation\frontend\dist")
Invoke-RobocopySafe -Source $modTemplateDir -Destination (Join-Path $releaseDir "services\web-workstation\mod_template") -ExcludeDirectories @(".godot", ".mono", "bin", "obj")
Copy-Item -LiteralPath (Join-Path $repoRoot "config.example.json") -Destination (Join-Path $releaseDir "services\web-workstation\config.example.json") -Force
Copy-Item -LiteralPath (Join-Path $templateRoot "workstation\Dockerfile") -Destination (Join-Path $releaseDir "services\web-workstation\Dockerfile") -Force
Copy-Item -LiteralPath (Join-Path $templateRoot "workstation\.dockerignore") -Destination (Join-Path $releaseDir "services\web-workstation\.dockerignore") -Force

Initialize-KnowledgeRuntimeDirs -ReleaseDir $releaseDir
Copy-RuntimeToolsBundle -ReleaseDir $releaseDir -RepoRoot $repoRoot
Copy-Item -LiteralPath (Join-Path $templateRoot "compose.app.yml") -Destination (Join-Path $releaseDir "docker-compose.yml") -Force
foreach ($scriptName in @("deploy-app.ps1", "stop-app.ps1", "package-app.ps1")) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $scriptName) -Destination (Join-Path $releaseDir "tools\latest\$scriptName") -Force
}
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination (Join-Path $releaseDir "README.md") -Force
Write-ReleaseManifest -ReleaseDir $releaseDir -RepoRoot $repoRoot

if (-not $SkipZip) {
    Write-Host "生成 zip 包..."
    New-ZipFromDirectory -SourceDir $releaseDir -DestinationPath $zipPath
}

Write-Host ""
Write-Host "打包完成:"
Write-Host "  Target      : app"
Write-Host "  Release 目录: $releaseDir"
if (-not $SkipZip) {
    Write-Host "  Zip 包      : $zipPath"
}
