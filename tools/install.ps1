<#
.SYNOPSIS
Windows 安装入口，统一安装 AgentTheSpire 运行与 Mod 开发依赖。

.DESCRIPTION
默认执行 .NET 9 SDK、Godot 4.5.1 Mono、backend/.venv、后端依赖、rembg 预热、frontend 依赖和前端构建。
传入 -OnlyModDeps 时，仅安装或配置 .NET 9 SDK 与 Godot 4.5.1 Mono。

.PARAMETER OnlyModDeps
只安装或配置 .NET 9 SDK 与 Godot 4.5.1 Mono。

.PARAMETER InstallLocalImage
额外安装 ComfyUI，并把本地图生配置写入 config.json。

.PARAMETER NonInteractive
禁用交互式结尾提示，适合脚本化调用。

.PARAMETER Help
显示帮助说明并退出。
#>
[CmdletBinding()]
param(
    [Parameter(HelpMessage = "只安装或配置 .NET 9 SDK 与 Godot 4.5.1 Mono。")]
    [Alias("ModOnly")]
    [switch]$OnlyModDeps,

    [Parameter(HelpMessage = "额外安装 ComfyUI，并写入本地图生配置。")]
    [Alias("LocalImage")]
    [switch]$InstallLocalImage,

    [Parameter(HelpMessage = "禁用交互式结尾提示，适合脚本化调用。")]
    [switch]$NonInteractive,

    [Parameter(HelpMessage = "显示帮助说明并退出。")]
    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

if ($Help) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

$script:RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$script:BackendDir = Join-Path $script:RootDir "backend"
$script:FrontendDir = Join-Path $script:RootDir "frontend"
$script:ConfigPath = Join-Path $script:RootDir "config.json"
$script:LogPath = Join-Path $script:RootDir ("install-{0}.log" -f (Get-Date -Format "yyyyMMdd-HHmmss"))
$script:GodotVersion = "4.5.1"
$script:GodotInstallDir = Join-Path $script:RootDir "godot"
$script:GodotExe = Join-Path $script:GodotInstallDir "Godot_v4.5.1-stable_mono_win64\Godot_v4.5.1-stable_mono_win64.exe"
$script:GodotDownloadUrl = "https://github.com/godotengine/godot/releases/download/4.5.1-stable/Godot_v4.5.1-stable_mono_win64.zip"
$script:DotNetInstallDir = Join-Path $env:LOCALAPPDATA "Microsoft\dotnet"
$script:DotNetInstallScriptUrl = "https://dot.net/v1/dotnet-install.ps1"

function Write-Section([string]$Title) {
    Write-Host ""
    Write-Host "== $Title ==" -ForegroundColor White
}

function Write-Info([string]$Message) {
    Write-Host ("[INFO {0}] {1}" -f (Get-Date -Format "HH:mm:ss"), $Message) -ForegroundColor Cyan
}

function Write-Ok([string]$Message) {
    Write-Host ("[OK] {0}" -f $Message) -ForegroundColor Green
}

function Write-Warn([string]$Message) {
    Write-Host ("[WARN] {0}" -f $Message) -ForegroundColor Yellow
}

function Test-InteractiveHost {
    return -not [Console]::IsInputRedirected
}

function Ensure-Directory([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        $null = New-Item -ItemType Directory -Path $Path -Force
    }
}

function Get-CommandSource([string[]]$Names) {
    foreach ($name in $Names) {
        $command = Get-Command $name -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $command) {
            return $command.Source
        }
    }
    return $null
}

function Get-ExternalCommandOutput {
    param(
        [string]$FilePath,
        [string[]]$Arguments = @(),
        [string]$WorkingDirectory = $script:RootDir
    )

    Push-Location $WorkingDirectory
    try {
        $output = & $FilePath @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }

    if ($exitCode -ne 0) {
        throw ("命令执行失败: {0} {1}`n{2}" -f $FilePath, ($Arguments -join " "), ($output | Out-String).Trim())
    }

    return ($output | Out-String).Trim()
}

function Invoke-ExternalCommand {
    param(
        [string]$FilePath,
        [string[]]$Arguments = @(),
        [string]$WorkingDirectory = $script:RootDir,
        [string]$FailureMessage
    )

    Push-Location $WorkingDirectory
    try {
        & $FilePath @Arguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }

    if ($exitCode -ne 0) {
        if ([string]::IsNullOrWhiteSpace($FailureMessage)) {
            throw ("命令执行失败: {0} {1} (exit {2})" -f $FilePath, ($Arguments -join " "), $exitCode)
        }
        throw ("{0} (exit {1})" -f $FailureMessage, $exitCode)
    }
}

function Test-VersionAtLeast([string]$ActualVersion, [Version]$MinimumVersion) {
    $normalized = ($ActualVersion -replace '[^0-9\.].*$', '')
    try {
        return ([Version]$normalized) -ge $MinimumVersion
    }
    catch {
        return $false
    }
}

function Add-UserPathEntry([string]$PathEntry) {
    if ([string]::IsNullOrWhiteSpace($PathEntry) -or -not (Test-Path -LiteralPath $PathEntry)) {
        return
    }

    $processEntries = @($env:Path -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($processEntries -notcontains $PathEntry) {
        $env:Path = (($processEntries + $PathEntry) -join ';')
    }

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $userEntries = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($userEntries -notcontains $PathEntry) {
        $updated = if ($userEntries.Count -gt 0) { ($userEntries + $PathEntry) -join ';' } else { $PathEntry }
        [Environment]::SetEnvironmentVariable("Path", $updated, "User")
    }
}

function Get-JsonConfigObject {
    if (-not (Test-Path -LiteralPath $script:ConfigPath)) {
        return [pscustomobject]@{}
    }
    $raw = Get-Content -LiteralPath $script:ConfigPath -Raw -Encoding UTF8
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return [pscustomobject]@{}
    }
    return $raw | ConvertFrom-Json
}

function Ensure-ObjectProperty($InputObject, [string]$Name, $DefaultValue) {
    if ($InputObject.PSObject.Properties.Name -notcontains $Name) {
        $InputObject | Add-Member -NotePropertyName $Name -NotePropertyValue $DefaultValue
    }
}

function Save-JsonConfigObject($Config) {
    Set-Content -LiteralPath $script:ConfigPath -Value ($Config | ConvertTo-Json -Depth 20) -Encoding UTF8
}

function Set-GodotPathInConfig([string]$GodotPath) {
    $config = Get-JsonConfigObject
    Ensure-ObjectProperty -InputObject $config -Name "godot_exe_path" -DefaultValue ""
    $config.godot_exe_path = $GodotPath
    Save-JsonConfigObject -Config $config
}

function Set-LocalImageConfig {
    $config = Get-JsonConfigObject
    Ensure-ObjectProperty -InputObject $config -Name "image_gen" -DefaultValue ([pscustomobject]@{})
    if ($config.image_gen -isnot [pscustomobject]) {
        $config.image_gen = [pscustomobject]@{}
    }
    $config.image_gen.local = [pscustomobject]@{
        comfyui_url = "http://127.0.0.1:8188"
        installed   = $true
        model_path   = ""
    }
    Save-JsonConfigObject -Config $config
}

function Ensure-Python {
    Write-Section "检查 Python"
    $pythonExe = Get-CommandSource -Names @("python.exe", "python")
    if (-not $pythonExe) {
        throw "未找到 Python，请先安装 Python 3.11+"
    }
    $versionOutput = Get-ExternalCommandOutput -FilePath $pythonExe -Arguments @("--version")
    $version = ($versionOutput -replace '^Python\s+', '').Trim()
    if (-not (Test-VersionAtLeast -ActualVersion $version -MinimumVersion ([Version]"3.11.0"))) {
        throw "Python 版本过低: $version，需要 Python 3.11+"
    }
    Write-Ok "Python $version"
    return $pythonExe
}

function Ensure-Node {
    Write-Section "检查 Node.js"
    $nodeExe = Get-CommandSource -Names @("node.exe", "node")
    $npmCmd = Get-CommandSource -Names @("npm.cmd", "npm")
    if (-not $nodeExe -or -not $npmCmd) {
        throw "未找到 Node.js/npm，请先安装 Node.js 18+"
    }
    $nodeVersion = (Get-ExternalCommandOutput -FilePath $nodeExe -Arguments @("--version")).TrimStart("v")
    if (-not (Test-VersionAtLeast -ActualVersion $nodeVersion -MinimumVersion ([Version]"18.0.0"))) {
        throw "Node.js 版本过低: $nodeVersion，需要 Node.js 18+"
    }
    Write-Ok "Node.js $nodeVersion"
    return @{ Node = $nodeExe; Npm = $npmCmd }
}

function Ensure-ClaudeCli {
    Write-Section "检查 Claude CLI"
    $claudeExe = Get-CommandSource -Names @("claude.cmd", "claude.exe", "claude")
    if ($claudeExe) {
        Write-Ok "claude CLI 已安装"
    } else {
        Write-Warn "未找到 claude CLI。订阅账号模式可运行: npm install -g @anthropic-ai/claude-code"
    }
}

function Get-DotNetVersion {
    $dotnetExe = Get-CommandSource -Names @("dotnet.exe", "dotnet")
    if (-not $dotnetExe) {
        $localDotNet = Join-Path $script:DotNetInstallDir "dotnet.exe"
        if (Test-Path -LiteralPath $localDotNet) {
            $dotnetExe = $localDotNet
        }
    }
    if (-not $dotnetExe) {
        return $null
    }
    return @{
        Executable = $dotnetExe
        Version    = (Get-ExternalCommandOutput -FilePath $dotnetExe -Arguments @("--version"))
    }
}

function Install-DotNetViaWinget {
    $wingetExe = Get-CommandSource -Names @("winget.exe", "winget")
    if (-not $wingetExe) {
        Write-Warn "未找到 winget，跳过 winget 安装 .NET 9"
        return $false
    }

    Write-Info "通过 winget 安装 .NET 9 SDK..."
    try {
        Invoke-ExternalCommand -FilePath $wingetExe -Arguments @(
            "install",
            "--id", "Microsoft.DotNet.SDK.9",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements"
        ) -FailureMessage ".NET 9 SDK winget 安装失败"
        return $true
    }
    catch {
        Write-Warn $_.Exception.Message
        return $false
    }
}

function Install-DotNetViaOfficialScript {
    Ensure-Directory -Path $script:DotNetInstallDir
    $tempScript = Join-Path $env:TEMP "AgentTheSpire-dotnet-install.ps1"
    Write-Info "通过官方 dotnet-install.ps1 安装 .NET 9 SDK 到用户目录..."
    Invoke-WebRequest -Uri $script:DotNetInstallScriptUrl -OutFile $tempScript
    try {
        Invoke-ExternalCommand -FilePath "powershell.exe" -Arguments @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", $tempScript,
            "-Channel", "9.0",
            "-InstallDir", $script:DotNetInstallDir
        ) -FailureMessage "官方脚本安装 .NET 9 SDK 失败"
    }
    finally {
        Remove-Item -LiteralPath $tempScript -Force -ErrorAction SilentlyContinue
    }
    Add-UserPathEntry -PathEntry $script:DotNetInstallDir
    Add-UserPathEntry -PathEntry (Join-Path $script:DotNetInstallDir "tools")
}

function Ensure-DotNetSdk9 {
    Write-Section "检查 .NET 9 SDK"
    $current = Get-DotNetVersion
    if ($current -and $current.Version -like "9.*") {
        Write-Ok ".NET SDK $($current.Version)"
        return $current.Executable
    }

    if ($current) {
        Write-Warn "当前 .NET 版本为 $($current.Version)，需要 9.x"
    } else {
        Write-Warn "未找到 dotnet，开始安装 .NET 9 SDK"
    }

    $installedByWinget = Install-DotNetViaWinget
    if (-not $installedByWinget) {
        Install-DotNetViaOfficialScript
    }

    $resolved = Get-DotNetVersion
    if (-not $resolved -or $resolved.Version -notlike "9.*") {
        throw "未能完成 .NET 9 SDK 安装，请手动安装后重试"
    }

    Write-Ok ".NET SDK $($resolved.Version)"
    return $resolved.Executable
}

function Find-GodotExecutable {
    $config = Get-JsonConfigObject
    $paths = @()
    if ($config.PSObject.Properties.Name -contains "godot_exe_path") {
        $paths += [string]$config.godot_exe_path
    }
    $paths += @(
        $script:GodotExe,
        "C:\Godot\Godot_v4.5.1-stable_mono_win64\Godot_v4.5.1-stable_mono_win64.exe",
        "C:\Program Files\Godot\Godot_v4.5.1-stable_mono_win64.exe",
        (Join-Path $env:USERPROFILE "Godot\Godot_v4.5.1-stable_mono_win64.exe")
    )
    foreach ($path in $paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) {
        if (Test-Path -LiteralPath $path) {
            return (Resolve-Path $path).Path
        }
    }
    return $null
}

function Install-Godot {
    Ensure-Directory -Path $script:GodotInstallDir
    $zipPath = Join-Path $env:TEMP "AgentTheSpire-godot-4.5.1-mono.zip"
    Write-Info "下载 Godot 4.5.1 Mono: $($script:GodotDownloadUrl)"
    Invoke-WebRequest -Uri $script:GodotDownloadUrl -OutFile $zipPath
    try {
        Expand-Archive -LiteralPath $zipPath -DestinationPath $script:GodotInstallDir -Force
    }
    finally {
        Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
    }
    if (-not (Test-Path -LiteralPath $script:GodotExe)) {
        throw "Godot 解压完成后找不到可执行文件: $($script:GodotExe)"
    }
    return (Resolve-Path $script:GodotExe).Path
}

function Ensure-Godot {
    Write-Section "检查 Godot 4.5.1 Mono"
    $godotPath = Find-GodotExecutable
    if (-not $godotPath) {
        Write-Warn "未找到 Godot 4.5.1 Mono，开始下载"
        $godotPath = Install-Godot
    }
    try {
        $versionOutput = Get-ExternalCommandOutput -FilePath $godotPath -Arguments @("--version", "--headless")
        if ($versionOutput -notmatch [regex]::Escape($script:GodotVersion)) {
            Write-Warn "无法自动确认 Godot 版本，请人工确认是 4.5.1: $versionOutput"
        } else {
            Write-Ok "Godot 版本验证通过: $($script:GodotVersion)"
        }
    }
    catch {
        Write-Warn "无法自动验证 Godot 版本，请人工确认是 4.5.1"
    }
    Set-GodotPathInConfig -GodotPath $godotPath
    Write-Ok "Godot 路径: $godotPath"
    return $godotPath
}

function Invoke-PipInstallWithFallback {
    param([string]$PythonExe, [string[]]$PipArguments)

    $sources = @()
    if (-not [string]::IsNullOrWhiteSpace($env:PIP_INDEX_URL)) {
        $sources += $env:PIP_INDEX_URL
    } else {
        $sources += @(
            "https://pypi.org/simple",
            "https://pypi.tuna.tsinghua.edu.cn/simple",
            "https://mirrors.aliyun.com/pypi/simple/",
            "https://pypi.mirrors.ustc.edu.cn/simple"
        )
    }

    foreach ($source in $sources) {
        Write-Info "pip 源: $source"
        Push-Location $script:BackendDir
        try {
            & $PythonExe -m pip install --disable-pip-version-check --default-timeout 60 --retries 2 --index-url $source @PipArguments
            $exitCode = $LASTEXITCODE
        }
        finally {
            Pop-Location
        }
        if ($exitCode -eq 0) {
            return
        }
        Write-Warn "pip 源失败: $source"
    }

    throw "所有可用 pip 源都失败，请检查网络或先设置 PIP_INDEX_URL 后重试"
}

function Ensure-BackendDependencies {
    param([string]$PythonExe)

    Write-Section "准备后端 Python 环境"
    $venvDir = Join-Path $script:BackendDir ".venv"
    $venvPython = Join-Path $venvDir "Scripts\python.exe"

    if (-not (Test-Path -LiteralPath $venvPython)) {
        Write-Info "创建 backend/.venv"
        Invoke-ExternalCommand -FilePath $PythonExe -Arguments @("-m", "venv", $venvDir) -FailureMessage "创建 backend/.venv 失败"
        Write-Ok "backend/.venv 创建完成"
    } else {
        Write-Ok "backend/.venv 已存在"
    }

    Write-Info "升级 pip"
    Invoke-PipInstallWithFallback -PythonExe $venvPython -PipArguments @("--upgrade", "pip")
    Write-Info "安装 backend/requirements.txt"
    Invoke-PipInstallWithFallback -PythonExe $venvPython -PipArguments @("-r", "requirements.txt")
    Write-Ok "后端依赖安装完成"

    Write-Info "预热 rembg 模型缓存"
    $tempScript = Join-Path $env:TEMP "AgentTheSpire-rembg-prewarm.py"
    @'
import json
import os
from pathlib import Path
from rembg import new_session

root = Path(os.environ["ROOT_DIR"])
cfg_path = root / "config.json"
raw = cfg_path.read_text(encoding="utf-8-sig") if cfg_path.exists() else ""
cfg = json.loads(raw) if raw.strip() else {}
model = cfg.get("image_gen", {}).get("rembg_model", "birefnet-general")
print(f"[INFO] rembg model: {model}")
new_session(model)
print("[OK] rembg model ready")
'@ | Set-Content -LiteralPath $tempScript -Encoding UTF8

    $previousRoot = $env:ROOT_DIR
    $env:ROOT_DIR = $script:RootDir
    try {
        try {
            Invoke-ExternalCommand -FilePath $venvPython -Arguments @($tempScript) -FailureMessage "rembg 模型预热失败"
            Write-Ok "rembg 模型已就绪"
        }
        catch {
            Write-Warn "rembg 模型预热失败，首次抠图时会自动下载"
        }
    }
    finally {
        $env:ROOT_DIR = $previousRoot
        Remove-Item -LiteralPath $tempScript -Force -ErrorAction SilentlyContinue
    }
}

function Ensure-FrontendDependencies {
    param([string]$NpmCmd)

    Write-Section "安装前端依赖"
    Invoke-ExternalCommand -FilePath $NpmCmd -Arguments @("install") -WorkingDirectory $script:FrontendDir -FailureMessage "前端依赖安装失败"
    Write-Ok "前端依赖安装完成"

    Write-Section "构建前端"
    Invoke-ExternalCommand -FilePath $NpmCmd -Arguments @("run", "build") -WorkingDirectory $script:FrontendDir -FailureMessage "前端构建失败"
    Write-Ok "前端构建完成"
}

function Install-LocalImageSupport {
    param([string]$PythonExe)

    Write-Section "安装本地图像生成组件"
    $gitExe = Get-CommandSource -Names @("git.exe", "git")
    if (-not $gitExe) {
        throw "安装 ComfyUI 需要 git，请先安装 Git for Windows"
    }

    $comfyUiDir = Join-Path $script:RootDir "comfyui"
    if (-not (Test-Path -LiteralPath $comfyUiDir)) {
        Invoke-ExternalCommand -FilePath $gitExe -Arguments @("clone", "https://github.com/comfyanonymous/ComfyUI.git", $comfyUiDir) -FailureMessage "克隆 ComfyUI 失败"
    } else {
        Write-Ok "ComfyUI 目录已存在，跳过克隆"
    }

    Invoke-ExternalCommand -FilePath $PythonExe -Arguments @("-m", "pip", "install", "-r", "requirements.txt") -WorkingDirectory $comfyUiDir -FailureMessage "安装 ComfyUI 依赖失败"
    Set-LocalImageConfig
    Write-Warn "FLUX.2 模型需手动下载到 comfyui\models\checkpoints\"
}

function Show-Summary {
    Write-Host ""
    Write-Host "==================================" -ForegroundColor White
    Write-Host "  AgentTheSpire 安装完成" -ForegroundColor Green
    Write-Host "  日志文件: $script:LogPath" -ForegroundColor Gray
    if ($OnlyModDeps) {
        Write-Host "  已完成: .NET 9 SDK / Godot 4.5.1 Mono" -ForegroundColor Gray
    } else {
        Write-Host "  已完成: .NET / Godot / backend/.venv / frontend build" -ForegroundColor Gray
    }
    Write-Host "==================================" -ForegroundColor White
    Write-Host ""
}

try {
    Start-Transcript -Path $script:LogPath -Force | Out-Null
}
catch {
    Write-Warn "无法启动 Transcript 日志，将继续执行。"
}

try {
    Write-Host ""
    Write-Host "==================================" -ForegroundColor White
    Write-Host "  AgentTheSpire Windows Installer" -ForegroundColor White
    Write-Host "==================================" -ForegroundColor White

    $pythonExe = $null
    $nodeTools = $null

    if (-not $OnlyModDeps) {
        $pythonExe = Ensure-Python
        $nodeTools = Ensure-Node
        Ensure-ClaudeCli
    }

    $null = Ensure-DotNetSdk9
    $null = Ensure-Godot

    if (-not $OnlyModDeps) {
        Ensure-BackendDependencies -PythonExe $pythonExe
        Ensure-FrontendDependencies -NpmCmd $nodeTools.Npm
        if ($InstallLocalImage) {
            Install-LocalImageSupport -PythonExe $pythonExe
        } else {
            Write-Info "未启用 -InstallLocalImage，跳过 ComfyUI 安装"
        }
    }

    Show-Summary
}
catch {
    Write-Host ""
    Write-Host ("[ERROR] {0}" -f $_.Exception.Message) -ForegroundColor Red
    Write-Host ("[INFO] 详细日志: {0}" -f $script:LogPath) -ForegroundColor Yellow
    if (-not $NonInteractive -and (Test-InteractiveHost)) {
        Write-Host ""
        Read-Host "按 Enter 退出"
    }
    exit 1
}
finally {
    try {
        Stop-Transcript | Out-Null
    }
    catch {
    }
}

if (-not $NonInteractive -and (Test-InteractiveHost)) {
    Read-Host "按 Enter 结束"
}
