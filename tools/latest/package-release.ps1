[CmdletBinding()]
param(
    [string]$OutputRoot = (Join-Path $PSScriptRoot "artifacts"),
    [string]$ReleaseName = "agentthespire-release",
    [switch]$SkipFrontendBuild,
    [switch]$SkipZip
)

$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
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

function Remove-DirectoryIfExists {
    param([string]$Path)

    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

function Remove-FileIfExists {
    param([string]$Path)

    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Force
    }
}

function New-CleanDirectory {
    param([string]$Path)

    Remove-DirectoryIfExists -Path $Path
    $null = New-Item -ItemType Directory -Path $Path -Force
}

function Invoke-RobocopySafe {
    param(
        [string]$Source,
        [string]$Destination,
        [string[]]$ExcludeDirectories = @(),
        [string[]]$ExcludeFiles = @()
    )

    $null = New-Item -ItemType Directory -Path $Destination -Force

    $arguments = @(
        $Source,
        $Destination,
        "/E",
        "/NFL",
        "/NDL",
        "/NJH",
        "/NJS",
        "/NP"
    )

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

function Write-ReleaseManifest {
    param(
        [string]$ReleaseDir,
        [string]$RepoRoot
    )

    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
    $manifest = @{
        release_name = Split-Path -Leaf $ReleaseDir
        created_at = (Get-Date).ToString("s")
        git_commit = $commit
        frontend_dist = "frontend/dist"
        backend_entry = "backend/main.py"
        compose_file = "docker-compose.yml"
    }

    $manifestPath = Join-Path $ReleaseDir "release-manifest.json"
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
}

$repoRoot = Get-RepoRoot
$frontendDir = Join-Path $repoRoot "frontend"
$frontendDistDir = Join-Path $frontendDir "dist"
$backendDir = Join-Path $repoRoot "backend"
$modTemplateDir = Join-Path $repoRoot "mod_template"
$templatesDir = Join-Path $PSScriptRoot "templates"
$releaseDir = Join-Path $OutputRoot $ReleaseName
$zipPath = Join-Path $OutputRoot ("{0}.zip" -f $ReleaseName)

Assert-PathExists -Path $frontendDir -Label "frontend 目录"
Assert-PathExists -Path $backendDir -Label "backend 目录"
Assert-PathExists -Path $modTemplateDir -Label "mod_template 目录"
Assert-PathExists -Path $templatesDir -Label "Docker 模板目录"

Assert-CommandExists -CommandName "git"
Assert-CommandExists -CommandName "robocopy"

if (-not $SkipFrontendBuild) {
    Assert-CommandExists -CommandName "node"
    Assert-CommandExists -CommandName "npm"

    Push-Location $frontendDir
    try {
        Write-Host "构建前端产物..."
        & npm run build
        if ($LASTEXITCODE -ne 0) {
            throw "前端构建失败"
        }
    }
    finally {
        Pop-Location
    }
}

Assert-PathExists -Path $frontendDistDir -Label "frontend/dist 构建产物"

$null = New-Item -ItemType Directory -Path $OutputRoot -Force
New-CleanDirectory -Path $releaseDir
$null = New-Item -ItemType Directory -Path (Join-Path $releaseDir "runtime") -Force
$null = New-Item -ItemType Directory -Path (Join-Path $releaseDir "frontend") -Force

Write-Host "整理 backend..."
Invoke-RobocopySafe -Source $backendDir -Destination (Join-Path $releaseDir "backend") -ExcludeDirectories @(
    ".venv",
    "tests",
    "__pycache__",
    ".pytest_cache"
) -ExcludeFiles @(
    "*.pyc",
    "2026-03-31-后端现状总结.md"
)
Remove-DirectoryIfExists -Path (Join-Path $releaseDir "backend\.tmp")
Remove-FileIfExists -Path (Join-Path $releaseDir "backend\2026-03-31-后端现状总结.md")

Write-Host "整理 frontend/dist..."
Invoke-RobocopySafe -Source $frontendDistDir -Destination (Join-Path $releaseDir "frontend\dist")

Write-Host "整理 mod_template..."
Invoke-RobocopySafe -Source $modTemplateDir -Destination (Join-Path $releaseDir "mod_template") -ExcludeDirectories @(
    ".godot",
    ".mono",
    "bin",
    "obj"
)

Copy-Item -LiteralPath (Join-Path $repoRoot "config.example.json") -Destination (Join-Path $releaseDir "config.example.json") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination (Join-Path $releaseDir "README.md") -Force
Copy-Item -LiteralPath (Join-Path $templatesDir "Dockerfile") -Destination (Join-Path $releaseDir "Dockerfile") -Force
Copy-Item -LiteralPath (Join-Path $templatesDir "docker-compose.yml") -Destination (Join-Path $releaseDir "docker-compose.yml") -Force
Copy-Item -LiteralPath (Join-Path $templatesDir ".dockerignore") -Destination (Join-Path $releaseDir ".dockerignore") -Force

Write-ReleaseManifest -ReleaseDir $releaseDir -RepoRoot $repoRoot

if (-not $SkipZip) {
    if (Test-Path -LiteralPath $zipPath) {
        Remove-Item -LiteralPath $zipPath -Force
    }
    Write-Host "生成 zip 包..."
    Compress-Archive -Path $releaseDir -DestinationPath $zipPath -Force
}

Write-Host ""
Write-Host "打包完成:"
Write-Host "  Release 目录: $releaseDir"
if (-not $SkipZip) {
    Write-Host "  Zip 包      : $zipPath"
}
