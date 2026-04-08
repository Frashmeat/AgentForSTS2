<#
.SYNOPSIS
按目标打包 AgentTheSpire 的 release bundle。

.DESCRIPTION
根据目标生成可部署的 release 目录，并可选输出 zip 包。
传入 PowerShell 内建的 `-Debug` 开关时，会在重新打包 `workstation` 相关目标时优先沿用旧 release 中已有的 `runtime/workstation.config.json`；若只检测到旧版 `services/workstation/config.json`，则作为兼容来源迁移；未传入时默认使用 `config.example.json` 生成新的 `runtime/workstation.config.json`。
直接执行脚本且不传任何参数时，会默认显示本帮助而不是立即开始打包。

.PARAMETER Target
打包目标。可选 full / hybrid / workstation / frontend / web。

.PARAMETER OutputRoot
输出目录。默认写入 tools/latest/artifacts。

.PARAMETER ReleaseName
发布目录名。默认按目标生成 agentthespire-<target>-release。

.PARAMETER SkipFrontendBuild
跳过前端构建。仅在已确认 frontend/dist 为最新时使用。

.PARAMETER SkipZip
跳过 zip 归档，只保留 release 目录。

.PARAMETER Help
显示帮助说明并退出。

.EXAMPLE
pwsh -File .\tools\latest\package-release.ps1 workstation

.EXAMPLE
pwsh -File .\tools\latest\package-release.ps1 -t web -NoZip

.EXAMPLE
pwsh -File .\tools\latest\package-release.ps1 workstation -Debug -NoZip
#>
[CmdletBinding()]
param(
    # 基础参数
    [Parameter(Position = 0, HelpMessage = "打包目标。可选 full / hybrid / workstation / frontend / web。")]
    [Alias("t")]
    [ValidateSet("full", "hybrid", "workstation", "frontend", "web")]
    [string]$Target = "workstation",

    [Parameter(HelpMessage = "输出目录。默认写入 tools/latest/artifacts。")]
    [Alias("o")]
    [string]$OutputRoot = "",

    [Parameter(HelpMessage = "发布目录名。默认按目标生成 agentthespire-<target>-release。")]
    [Alias("n")]
    [string]$ReleaseName = "",

    # 行为开关
    [Parameter(HelpMessage = "跳过前端构建。仅在已确认 frontend/dist 为最新时使用。")]
    [Alias("NoFrontend")]
    [switch]$SkipFrontendBuild,

    [Parameter(HelpMessage = "跳过 zip 归档，只保留 release 目录。")]
    [Alias("NoZip")]
    [switch]$SkipZip,

    [Parameter(HelpMessage = "显示帮助说明并退出。")]
    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help -or $PSBoundParameters.Count -eq 0) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $PSScriptRoot "artifacts"
}

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
    param(
        [string]$Path,
        [string[]]$PreserveRelativePaths = @()
    )

    if ($PreserveRelativePaths.Count -eq 0) {
        Remove-DirectoryIfExists -Path $Path
        $null = New-Item -ItemType Directory -Path $Path -Force
        return
    }

    $null = New-Item -ItemType Directory -Path $Path -Force

    foreach ($child in Get-ChildItem -LiteralPath $Path -Force) {
        $relativePath = [System.IO.Path]::GetRelativePath($Path, $child.FullName)
        $shouldPreserve = $false

        foreach ($preservedRelativePath in $PreserveRelativePaths) {
            if (
                $relativePath -eq $preservedRelativePath -or
                $relativePath.StartsWith("$preservedRelativePath\") -or
                $preservedRelativePath.StartsWith("$relativePath\")
            ) {
                $shouldPreserve = $true
                break
            }
        }

        if ($shouldPreserve) {
            continue
        }

        Remove-Item -LiteralPath $child.FullName -Recurse -Force
    }
}

function Test-FrontendDependenciesReady {
    param([string]$FrontendDir)

    $packageJsonPath = Join-Path $FrontendDir "package.json"
    $packageLockPath = Join-Path $FrontendDir "package-lock.json"
    $nodeModulesDir = Join-Path $FrontendDir "node_modules"

    if (-not (Test-Path -LiteralPath $packageJsonPath)) {
        throw "缺少 frontend/package.json: $packageJsonPath"
    }

    if ((-not (Test-Path -LiteralPath $packageLockPath)) -or (-not (Test-Path -LiteralPath $nodeModulesDir))) {
        return $false
    }

    $packageJson = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json -AsHashtable
    $requiredPackages = @()

    if ($packageJson.ContainsKey("dependencies")) {
        $requiredPackages += $packageJson.dependencies.Keys
    }

    if ($packageJson.ContainsKey("devDependencies")) {
        $requiredPackages += $packageJson.devDependencies.Keys
    }

    foreach ($packageName in $requiredPackages | Sort-Object -Unique) {
        $packagePath = Join-Path $nodeModulesDir ($packageName -replace "/", "\")
        if (-not (Test-Path -LiteralPath $packagePath)) {
            Write-Host "检测到缺失前端依赖: $packageName"
            return $false
        }
    }

    $packageJsonTime = (Get-Item -LiteralPath $packageJsonPath).LastWriteTimeUtc
    $packageLockTime = (Get-Item -LiteralPath $packageLockPath).LastWriteTimeUtc
    return $packageLockTime -ge $packageJsonTime
}

function Ensure-FrontendDependencies {
    param([string]$FrontendDir)

    if (Test-FrontendDependenciesReady -FrontendDir $FrontendDir) {
        return
    }

    Write-Host "检测到前端依赖缺失或锁文件落后，执行 npm install..."
    & npm install
    if ($LASTEXITCODE -ne 0) {
        throw "前端依赖安装失败"
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
    # robocopy 的 0-7 都属于成功或可接受的差异状态，8 以上才表示失败。
    if ($LASTEXITCODE -gt 7) {
        throw "robocopy 失败，退出码: $LASTEXITCODE"
    }
}

function Get-ServiceDefinitions {
    param([string]$SelectedTarget)

    # 每个目标都映射为一组需要打进 release bundle 的服务定义。
    # RemovePaths 用来剔除当前目标不应暴露的入口和路由，避免把整仓库原样塞进镜像。
    switch ($SelectedTarget) {
        "full" {
            return @(
                @{ Name = "workstation"; IncludeBackend = $true; IncludeFrontend = $true; IncludeModTemplate = $true; TemplateDir = "workstation"; RemovePaths = @("backend/main_web.py", "backend/routers/platform_admin.py", "backend/routers/platform_jobs.py") },
                @{ Name = "web"; IncludeBackend = $true; IncludeFrontend = $false; IncludeModTemplate = $false; TemplateDir = "web"; RemovePaths = @("backend/main_workstation.py", "backend/routers/workflow.py", "backend/routers/config_router.py", "backend/routers/batch_workflow.py", "backend/routers/log_analyzer.py", "backend/routers/mod_analyzer.py", "backend/routers/build_deploy.py", "backend/routers/approval_router.py") }
            )
        }
        "hybrid" {
            return @(
                @{ Name = "workstation"; IncludeBackend = $true; IncludeFrontend = $true; IncludeModTemplate = $true; TemplateDir = "workstation"; RemovePaths = @("backend/main_web.py", "backend/routers/platform_admin.py", "backend/routers/platform_jobs.py") },
                @{ Name = "frontend"; IncludeBackend = $false; IncludeFrontend = $true; IncludeModTemplate = $false; TemplateDir = "frontend"; RemovePaths = @() }
            )
        }
        "workstation" {
            return @(
                @{ Name = "workstation"; IncludeBackend = $true; IncludeFrontend = $true; IncludeModTemplate = $true; TemplateDir = "workstation"; RemovePaths = @("backend/main_web.py", "backend/routers/platform_admin.py", "backend/routers/platform_jobs.py") }
            )
        }
        "frontend" {
            return @(
                @{ Name = "frontend"; IncludeBackend = $false; IncludeFrontend = $true; IncludeModTemplate = $false; TemplateDir = "frontend"; RemovePaths = @() }
            )
        }
        "web" {
            return @(
                @{ Name = "web"; IncludeBackend = $true; IncludeFrontend = $false; IncludeModTemplate = $false; TemplateDir = "web"; RemovePaths = @("backend/main_workstation.py", "backend/routers/workflow.py", "backend/routers/config_router.py", "backend/routers/batch_workflow.py", "backend/routers/log_analyzer.py", "backend/routers/mod_analyzer.py", "backend/routers/build_deploy.py", "backend/routers/approval_router.py") }
            )
        }
        default {
            throw "未知 Target: $SelectedTarget"
        }
    }
}

function Copy-ServiceBundle {
    param(
        [hashtable]$Service,
        [string]$ReleaseDir,
        [string]$RepoRoot,
        [string]$BackendDir,
        [string]$FrontendDistDir,
        [string]$ModTemplateDir,
        [string]$TemplatesDir,
        [hashtable]$PreviousServiceConfigs = @{},
        [switch]$ReuseExistingSettings
    )

    $serviceDir = Join-Path (Join-Path $ReleaseDir "services") $Service.Name
    New-CleanDirectory -Path $serviceDir

    if ($Service.IncludeBackend) {
        Write-Host "整理 backend -> $($Service.Name)..."
        Invoke-RobocopySafe -Source $BackendDir -Destination (Join-Path $serviceDir "backend") -ExcludeDirectories @(
            ".venv",
            ".tmp",
            "tests",
            "__pycache__",
            ".pytest_cache"
        ) -ExcludeFiles @(
            "*.pyc",
            "2026-03-31-后端现状总结.md"
        )
    }

    if ($Service.IncludeFrontend) {
        # 这里只复制已经构建好的 dist，运行时不再依赖 Node 环境。
        Write-Host "整理 frontend/dist -> $($Service.Name)..."
        Invoke-RobocopySafe -Source $FrontendDistDir -Destination (Join-Path $serviceDir "frontend\dist")
    }

    if ($Service.IncludeModTemplate) {
        Write-Host "整理 mod_template -> $($Service.Name)..."
        Invoke-RobocopySafe -Source $ModTemplateDir -Destination (Join-Path $serviceDir "mod_template") -ExcludeDirectories @(
            ".godot",
            ".mono",
            "bin",
            "obj"
        )
    }

    $templateRoot = Join-Path $TemplatesDir $Service.TemplateDir
    Copy-Item -LiteralPath (Join-Path $templateRoot "Dockerfile") -Destination (Join-Path $serviceDir "Dockerfile") -Force
    Copy-Item -LiteralPath (Join-Path $templateRoot ".dockerignore") -Destination (Join-Path $serviceDir ".dockerignore") -Force

    if ($Service.TemplateDir -eq "frontend") {
        Copy-Item -LiteralPath (Join-Path $templateRoot "nginx.conf") -Destination (Join-Path $serviceDir "nginx.conf") -Force
    }

    if ($Service.IncludeBackend) {
        Copy-Item -LiteralPath (Join-Path $RepoRoot "config.example.json") -Destination (Join-Path $serviceDir "config.example.json") -Force

        if ($Service.Name -eq "workstation") {
            $configTargetPath = Join-Path (Join-Path $ReleaseDir "runtime") "workstation.config.json"
            $configContent = ""
            if ($ReuseExistingSettings.IsPresent -and $PreviousServiceConfigs.ContainsKey($Service.Name)) {
                $configContent = [string]$PreviousServiceConfigs[$Service.Name]
            }
            if ([string]::IsNullOrWhiteSpace($configContent)) {
                $configContent = Get-Content -LiteralPath (Join-Path $RepoRoot "config.example.json") -Raw
            }
            Set-Content -LiteralPath $configTargetPath -Value $configContent -Encoding UTF8
        }
    }

    foreach ($relativePath in $Service.RemovePaths) {
        $targetPath = Join-Path $serviceDir $relativePath
        if (Test-Path -LiteralPath $targetPath) {
            Remove-Item -LiteralPath $targetPath -Recurse -Force
        }
    }
}

function Copy-LauncherBundle {
    param(
        [string]$ReleaseDir,
        [string]$RepoRoot,
        [string]$TargetName
    )

    if ($TargetName -notin @("workstation", "hybrid")) {
        return
    }

    $launcherDir = Join-Path $ReleaseDir "launcher"
    $null = New-Item -ItemType Directory -Path $launcherDir -Force

    foreach ($relativePath in @(
        "tools\split-local\start_split_local.ps1",
        "tools\split-local\start_split_local.bat",
        "tools\split-local\stop_split_local.ps1",
        "tools\split-local\stop_split_local.bat"
    )) {
        Copy-Item -LiteralPath (Join-Path $RepoRoot $relativePath) -Destination (Join-Path $launcherDir (Split-Path $relativePath -Leaf)) -Force
    }
}

function Write-ReleaseManifest {
    param(
        [string]$ReleaseDir,
        [string]$RepoRoot,
        [string]$SelectedTarget,
        [array]$Services
    )

    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
    $manifest = @{
        release_name = Split-Path -Leaf $ReleaseDir
        target = $SelectedTarget
        created_at = (Get-Date).ToString("s")
        git_commit = $commit
        services = [object[]]@($Services | ForEach-Object { $_.Name })
        compose_file = "docker-compose.yml"
    }

    $manifestPath = Join-Path $ReleaseDir "release-manifest.json"
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
}

function Get-PreviousServiceConfigs {
    param([string]$ExistingReleaseDir)

    $configs = @{}
    $workstationConfigPath = Join-Path $ExistingReleaseDir "runtime\workstation.config.json"
    if (Test-Path -LiteralPath $workstationConfigPath) {
        $configs["workstation"] = Get-Content -LiteralPath $workstationConfigPath -Raw
        return $configs
    }

    $legacyWorkstationConfigPath = Join-Path $ExistingReleaseDir "services\workstation\config.json"
    if (Test-Path -LiteralPath $legacyWorkstationConfigPath) {
        $configs["workstation"] = Get-Content -LiteralPath $legacyWorkstationConfigPath -Raw
    }
    return $configs
}

$repoRoot = Get-RepoRoot
$frontendDir = Join-Path $repoRoot "frontend"
$frontendDistDir = Join-Path $frontendDir "dist"
$backendDir = Join-Path $repoRoot "backend"
$modTemplateDir = Join-Path $repoRoot "mod_template"
$templatesDir = Join-Path $PSScriptRoot "templates"
$effectiveReleaseName = if ([string]::IsNullOrWhiteSpace($ReleaseName)) { "agentthespire-$Target-release" } else { $ReleaseName }
$releaseDir = Join-Path $OutputRoot $effectiveReleaseName
$zipPath = Join-Path $OutputRoot ("{0}.zip" -f $effectiveReleaseName)
$composeTemplate = Join-Path $templatesDir ("compose.{0}.yml" -f $Target)
$serviceDefinitions = Get-ServiceDefinitions -SelectedTarget $Target
$reuseExistingSettings = $PSBoundParameters.ContainsKey("Debug")
$needsBackend = @($serviceDefinitions | Where-Object { $_.IncludeBackend }).Count -gt 0
$needsFrontend = @($serviceDefinitions | Where-Object { $_.IncludeFrontend }).Count -gt 0
$needsModTemplate = @($serviceDefinitions | Where-Object { $_.IncludeModTemplate }).Count -gt 0
$previousServiceConfigs = if ($reuseExistingSettings -and (Test-Path -LiteralPath $releaseDir)) {
    Get-PreviousServiceConfigs -ExistingReleaseDir $releaseDir
} else {
    @{}
}

Assert-PathExists -Path $templatesDir -Label "模板目录"
Assert-PathExists -Path $composeTemplate -Label "compose 模板"

Assert-CommandExists -CommandName "git"
Assert-CommandExists -CommandName "robocopy"

if ($needsFrontend) {
    Assert-PathExists -Path $frontendDir -Label "frontend 目录"
}

if ($needsBackend) {
    Assert-PathExists -Path $backendDir -Label "backend 目录"
}

if ($needsModTemplate) {
    Assert-PathExists -Path $modTemplateDir -Label "mod_template 目录"
}

if ($needsFrontend -and (-not $SkipFrontendBuild)) {
    Assert-CommandExists -CommandName "node"
    Assert-CommandExists -CommandName "npm"

    Push-Location $frontendDir
    try {
        Ensure-FrontendDependencies -FrontendDir $frontendDir

        # 统一在打包阶段产出 dist，避免部署脚本再触发前端构建。
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

if ($needsFrontend) {
    Assert-PathExists -Path $frontendDistDir -Label "frontend/dist 构建产物"
}

$null = New-Item -ItemType Directory -Path $OutputRoot -Force
New-CleanDirectory -Path $releaseDir -PreserveRelativePaths @("runtime\logs", "runtime\python-runtime")
$null = New-Item -ItemType Directory -Path (Join-Path $releaseDir "services") -Force
$null = New-Item -ItemType Directory -Path (Join-Path $releaseDir "runtime") -Force

foreach ($service in $serviceDefinitions) {
    Copy-ServiceBundle -Service $service -ReleaseDir $releaseDir -RepoRoot $repoRoot -BackendDir $backendDir -FrontendDistDir $frontendDistDir -ModTemplateDir $modTemplateDir -TemplatesDir $templatesDir -PreviousServiceConfigs $previousServiceConfigs -ReuseExistingSettings:$reuseExistingSettings
}
Copy-LauncherBundle -ReleaseDir $releaseDir -RepoRoot $repoRoot -TargetName $Target

Copy-Item -LiteralPath $composeTemplate -Destination (Join-Path $releaseDir "docker-compose.yml") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination (Join-Path $releaseDir "README.md") -Force
Write-ReleaseManifest -ReleaseDir $releaseDir -RepoRoot $repoRoot -SelectedTarget $Target -Services $serviceDefinitions

if (-not $SkipZip) {
    if (Test-Path -LiteralPath $zipPath) {
        Remove-Item -LiteralPath $zipPath -Force
    }
    Write-Host "生成 zip 包..."
    Compress-Archive -Path $releaseDir -DestinationPath $zipPath -Force
}

Write-Host ""
Write-Host "打包完成:"
Write-Host "  Target      : $Target"
Write-Host "  Release 目录: $releaseDir"
Write-Host "  沿用旧设置  : $reuseExistingSettings"
if (-not $SkipZip) {
    Write-Host "  Zip 包      : $zipPath"
}
