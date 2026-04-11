<#
.SYNOPSIS
统一的 tools 目录入口。

.DESCRIPTION
支持两种使用方式：
1. 直接运行脚本，进入分层数字菜单，使用键盘选择要执行的脚本和参数模板
2. 通过参数直达具体脚本，例如 install / start / split / dev / latest

.EXAMPLE
powershell -File .\tools\tools.ps1

.EXAMPLE
powershell -File .\tools\tools.ps1 install mod

.EXAMPLE
powershell -File .\tools\tools.ps1 start workstation

.EXAMPLE
powershell -File .\tools\tools.ps1 split start -DryRun

.EXAMPLE
powershell -File .\tools\tools.ps1 latest package workstation

.EXAMPLE
powershell -File .\tools\tools.ps1 latest package hybrid
#>
param(
    [Parameter(Position = 0)]
    [string]$Group = "",

    [Parameter(Position = 1)]
    [string]$Action = "",

    [switch]$Help,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

function Get-PowerShellCommandPath {
    $powershellCommand = Get-Command powershell -ErrorAction SilentlyContinue
    if ($null -eq $powershellCommand) {
        throw "未找到 powershell 命令。"
    }
    return $powershellCommand.Source
}

function Invoke-TargetScript {
    param(
        [string]$Path,
        [string[]]$Arguments = @()
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "未找到脚本：$Path"
    }

    $extension = [System.IO.Path]::GetExtension($Path).ToLowerInvariant()
    switch ($extension) {
        ".ps1" {
            & (Get-PowerShellCommandPath) -NoProfile -ExecutionPolicy Bypass -File $Path @Arguments
            return
        }
        ".bat" {
            & $Path @Arguments
            return
        }
        ".cmd" {
            & $Path @Arguments
            return
        }
        ".sh" {
            & "bash" $Path @Arguments
            return
        }
        ".py" {
            $python = Get-Command python -ErrorAction SilentlyContinue
            if ($null -eq $python) {
                throw "未找到 python，无法执行：$Path"
            }
            & $python.Source $Path @Arguments
            return
        }
        default {
            throw "不支持的脚本类型：$Path"
        }
    }
}

function Show-Help {
    Write-Host ""
    Write-Host "AgentTheSpire tools 统一入口" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "直接运行："
    Write-Host "  powershell -File .\tools\tools.ps1"
    Write-Host "  进入分层数字菜单，可直接用键盘选择功能和参数模板。"
    Write-Host ""
    Write-Host "参数直达："
    Write-Host "  install                安装完整运行依赖"
    Write-Host "  install mod            只安装 Mod 开发依赖"
    Write-Host "  start workstation      启动 workstation-backend"
    Write-Host "  start web              启动 web-backend"
    Write-Host "  start dev              启动开发模式"
    Write-Host "  split start            启动独立前端 + 本地 workstation"
    Write-Host "  split stop             停止 split-local 双进程"
    Write-Host "  dev decompile          反编译 sts2.dll"
    Write-Host "  latest package <target> 打包 release bundle"
    Write-Host "  latest deploy <target>  部署 mixed release（web 用 Docker，其余目标按需本机启动）"
    Write-Host "  latest installer       构建 workstation 安装器"
    Write-Host ""
    Write-Host "示例："
    Write-Host "  powershell -File .\tools\tools.ps1 install"
    Write-Host "  powershell -File .\tools\tools.ps1 install mod"
    Write-Host "  powershell -File .\tools\tools.ps1 split start -DryRun"
    Write-Host "  powershell -File .\tools\tools.ps1 latest package hybrid"
    Write-Host "  powershell -File .\tools\tools.ps1 latest package workstation"
    Write-Host "  powershell -File .\tools\tools.ps1 latest deploy hybrid"
    Write-Host "  powershell -File .\tools\tools.ps1 latest deploy hybrid -WebBaseUrl https://your-web-api.example.com"
    Write-Host ""
}

function Test-InteractiveConsole {
    try {
        return (-not [Console]::IsInputRedirected) -and (-not [Console]::IsOutputRedirected)
    }
    catch {
        return $true
    }
}

function New-MenuProfile {
    param(
        [string]$Key,
        [string]$Label,
        [string]$Description,
        [string[]]$Args = @(),
        [string]$PromptHandler = ""
    )

    return [pscustomobject]@{
        Key = $Key
        Label = $Label
        Description = $Description
        Args = $Args
        PromptHandler = $PromptHandler
    }
}

function New-MenuCommand {
    param(
        [string]$Key,
        [string]$Label,
        [string]$Description,
        [string]$ScriptPath,
        [string]$InvocationName,
        [string[]]$DefaultArgs = @(),
        [object[]]$Profiles = @()
    )

    return [pscustomobject]@{
        Key = $Key
        Label = $Label
        Description = $Description
        ScriptPath = $ScriptPath
        InvocationName = $InvocationName
        DefaultArgs = $DefaultArgs
        Profiles = $Profiles
    }
}

function Read-MenuChoice {
    param(
        [string]$Title,
        [object[]]$Options,
        [switch]$AllowBack,
        [switch]$AllowQuit
    )

    while ($true) {
        Write-Host ""
        Write-Host $Title -ForegroundColor Cyan
        Write-Host ""

        for ($index = 0; $index -lt $Options.Count; $index += 1) {
            $number = $index + 1
            $option = $Options[$index]
            Write-Host ("  {0}. {1}" -f $number, $option.Label)
            if (-not [string]::IsNullOrWhiteSpace($option.Description)) {
                Write-Host ("     {0}" -f $option.Description) -ForegroundColor DarkGray
            }
        }

        if ($AllowBack) {
            Write-Host "  B. 返回上一级"
        }
        if ($AllowQuit) {
            Write-Host "  Q. 退出"
        }

        $rawChoice = Read-Host "请选择"
        if ([string]::IsNullOrWhiteSpace($rawChoice)) {
            continue
        }

        $normalized = $rawChoice.Trim()
        if ($AllowBack -and $normalized.Equals("B", [System.StringComparison]::OrdinalIgnoreCase)) {
            return "__back__"
        }
        if ($AllowQuit -and $normalized.Equals("Q", [System.StringComparison]::OrdinalIgnoreCase)) {
            return "__quit__"
        }

        $number = 0
        if ([int]::TryParse($normalized, [ref]$number)) {
            if ($number -ge 1 -and $number -le $Options.Count) {
                return $Options[$number - 1]
            }
        }

        Write-Host "输入无效，请重新选择。" -ForegroundColor Yellow
    }
}

function Get-SplitStartCustomArgs {
    $args = @()

    $workstationPort = Read-Host "Workstation 端口（直接回车使用默认 7860）"
    if (-not [string]::IsNullOrWhiteSpace($workstationPort)) {
        $args += @("-WorkstationPort", $workstationPort.Trim())
    }

    $frontendPort = Read-Host "前端静态端口（直接回车使用默认 8080）"
    if (-not [string]::IsNullOrWhiteSpace($frontendPort)) {
        $args += @("-FrontendPort", $frontendPort.Trim())
    }

    $webBaseUrl = Read-Host "Web API 基地址（直接回车使用默认 http://127.0.0.1:7870）"
    if (-not [string]::IsNullOrWhiteSpace($webBaseUrl)) {
        $args += @("-WebBaseUrl", $webBaseUrl.Trim())
    }

    $noBrowser = Read-Host "是否不自动打开浏览器？(y/N)"
    if ($noBrowser -match '^(?i:y|yes)$') {
        $args += "-NoBrowser"
    }

    $dryRun = Read-Host "是否只做 DryRun 预览？(y/N)"
    if ($dryRun -match '^(?i:y|yes)$') {
        $args += "-DryRun"
    }

    return $args
}

function Get-LatestDeployHybridArgs {
    $webBaseUrl = Read-Host "Web API 基地址（直接回车使用默认 http://127.0.0.1:7870）"
    if (-not [string]::IsNullOrWhiteSpace($webBaseUrl)) {
        return @("-WebBaseUrl", $webBaseUrl.Trim())
    }

    return @()
}

function Resolve-ProfileArgs {
    param(
        $Command,
        $Profile
    )

    if ([string]::IsNullOrWhiteSpace($Profile.PromptHandler)) {
        return $Profile.Args
    }

    switch ($Profile.PromptHandler) {
        "SplitStartCustom" {
            return Get-SplitStartCustomArgs
        }
        "LatestDeployHybrid" {
            return Get-LatestDeployHybridArgs
        }
        default {
            throw "未支持的参数提示处理器：$($Profile.PromptHandler)"
        }
    }
}

function Confirm-And-InvokeMenuCommand {
    param(
        $Command,
        $Profile
    )

    $resolvedArgs = @($Command.DefaultArgs + (Resolve-ProfileArgs -Command $Command -Profile $Profile))
    $previewArgs = if ($resolvedArgs.Count -gt 0) { $resolvedArgs -join " " } else { "(无参数)" }

    Write-Host ""
    Write-Host "将执行：" -ForegroundColor Cyan
    Write-Host ("  功能       : {0}" -f $Command.Label)
    Write-Host ("  参数模板   : {0}" -f $Profile.Label)
    Write-Host ("  脚本路径   : {0}" -f $Command.ScriptPath)
    Write-Host ("  命令预览   : powershell -File .\tools\tools.ps1 {0}" -f $Command.InvocationName)
    Write-Host ("  实际参数   : {0}" -f $previewArgs)
    Write-Host ""

    $confirm = Read-Host "确认执行？(Y/N)"
    if ($confirm -notmatch '^(?i:y|yes)$') {
        Write-Host "已取消执行。" -ForegroundColor Yellow
        return
    }

    Invoke-TargetScript -Path $Command.ScriptPath -Arguments $resolvedArgs
}

function Get-MenuGroups {
    $toolsRoot = $PSScriptRoot

    return @(
        [pscustomobject]@{
            Key = "install"
            Label = "安装"
            Description = "安装环境与运行依赖"
            Items = @(
                (New-MenuCommand -Key "install-full" -Label "完整安装" -Description "安装 .NET / Godot / 后端依赖 / 前端依赖 / 前端构建" -ScriptPath (Join-Path $toolsRoot "install\install.ps1") -InvocationName "install" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认流程完整安装")
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 install.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "install-mod" -Label "只安装 Mod 依赖" -Description "只安装 .NET 9 和 Godot 4.5.1 Mono" -ScriptPath (Join-Path $toolsRoot "install\install.ps1") -InvocationName "install mod" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "只安装或配置 Mod 开发依赖" -Args @("-OnlyModDeps"))
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 install.ps1 参数说明" -Args @("-Help"))
                ))
            )
        }
        [pscustomobject]@{
            Key = "start"
            Label = "启动"
            Description = "启动 workstation / web / dev"
            Items = @(
                (New-MenuCommand -Key "start-workstation" -Label "启动 workstation-backend" -Description "启动本地工作站后端" -ScriptPath (Join-Path $toolsRoot "start\start_workstation.bat") -InvocationName "start workstation" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认方式启动 workstation-backend")
                ))
                (New-MenuCommand -Key "start-web" -Label "启动 web-backend" -Description "启动平台 Web API 后端" -ScriptPath (Join-Path $toolsRoot "start\start_web.bat") -InvocationName "start web" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认方式启动 web-backend")
                ))
                (New-MenuCommand -Key "start-dev" -Label "启动开发模式" -Description "拉起后端热重载和 Vite 前端开发服务器" -ScriptPath (Join-Path $toolsRoot "start\start_dev.bat") -InvocationName "start dev" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认方式启动开发模式")
                ))
            )
        }
        [pscustomobject]@{
            Key = "split"
            Label = "拆分运行时"
            Description = "独立前端 + 本地 workstation 的双进程形态"
            Items = @(
                (New-MenuCommand -Key "split-start" -Label "启动 split-local" -Description "启动独立前端 + 本地 workstation" -ScriptPath (Join-Path $toolsRoot "split-local\start_split_local.ps1") -InvocationName "split start" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "默认启动" -Description "使用默认端口直接启动")
                    (New-MenuProfile -Key "dryrun" -Label "DryRun 预览" -Description "只打印布局和端口，不真正启动" -Args @("-DryRun"))
                    (New-MenuProfile -Key "nobrowser" -Label "启动但不打开浏览器" -Description "适合后台或远程环境" -Args @("-NoBrowser"))
                    (New-MenuProfile -Key "custom" -Label "自定义端口/地址" -Description "逐项输入自定义端口和 Web API 地址" -PromptHandler "SplitStartCustom")
                ))
                (New-MenuCommand -Key "split-stop" -Label "停止 split-local" -Description "停止 split-local 双进程并清理状态文件" -ScriptPath (Join-Path $toolsRoot "split-local\stop_split_local.ps1") -InvocationName "split stop" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "停止已启动的 split-local 进程")
                ))
            )
        }
        [pscustomobject]@{
            Key = "dev"
            Label = "开发辅助"
            Description = "反编译游戏 DLL 等开发辅助"
            Items = @(
                (New-MenuCommand -Key "dev-decompile" -Label "反编译 sts2.dll" -Description "运行 decompile_sts2.py" -ScriptPath (Join-Path $toolsRoot "dev\decompile_sts2.py") -InvocationName "dev decompile" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接执行" -Description "按当前配置运行反编译脚本")
                ))
            )
        }
        [pscustomobject]@{
            Key = "latest"
            Label = "打包 / 部署"
            Description = "统一调度 tools/latest 下的发布脚本"
            Items = @(
                (New-MenuCommand -Key "latest-package-workstation" -Label "打包 workstation release" -Description "构建前端 + workstation release bundle" -ScriptPath (Join-Path $toolsRoot "latest\package-release.ps1") -InvocationName "latest package workstation" -DefaultArgs @("workstation") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接打包" -Description "按默认参数打包 workstation")
                    (New-MenuProfile -Key "nozip" -Label "打包但不压缩" -Description "保留 release 目录，跳过 zip" -Args @("-NoZip"))
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 package-release.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-package-hybrid" -Label "打包 hybrid release" -Description "构建正式推荐的 frontend + workstation 用户侧 bundle" -ScriptPath (Join-Path $toolsRoot "latest\package-release.ps1") -InvocationName "latest package hybrid" -DefaultArgs @("hybrid") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接打包" -Description "按默认参数打包 hybrid")
                    (New-MenuProfile -Key "nozip" -Label "打包但不压缩" -Description "保留 release 目录，跳过 zip" -Args @("-NoZip"))
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 package-release.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-package-frontend" -Label "打包 frontend release" -Description "只打前端静态站点 release" -ScriptPath (Join-Path $toolsRoot "latest\package-release.ps1") -InvocationName "latest package frontend" -DefaultArgs @("frontend") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接打包" -Description "按默认参数打包 frontend")
                    (New-MenuProfile -Key "nozip" -Label "打包但不压缩" -Description "保留 release 目录，跳过 zip" -Args @("-NoZip"))
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 package-release.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-package-web" -Label "打包 web release" -Description "只打 Web API release" -ScriptPath (Join-Path $toolsRoot "latest\package-release.ps1") -InvocationName "latest package web" -DefaultArgs @("web") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接打包" -Description "按默认参数打包 web")
                    (New-MenuProfile -Key "nozip" -Label "打包但不压缩" -Description "保留 release 目录，跳过 zip" -Args @("-NoZip"))
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 package-release.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-deploy-workstation" -Label "部署 workstation release" -Description "在本机启动 workstation-backend" -ScriptPath (Join-Path $toolsRoot "latest\deploy-docker.ps1") -InvocationName "latest deploy workstation" -DefaultArgs @("workstation") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接部署" -Description "按默认参数在本机启动 workstation")
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 deploy-docker.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-deploy-hybrid" -Label "部署 hybrid release" -Description "部署正式推荐的 frontend + workstation 用户侧 bundle" -ScriptPath (Join-Path $toolsRoot "latest\deploy-docker.ps1") -InvocationName "latest deploy hybrid" -DefaultArgs @("hybrid") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "配置 Web API 后部署" -Description "可留空使用本机 web-backend，也可填写远端地址" -PromptHandler "LatestDeployHybrid")
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 deploy-docker.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-deploy-frontend" -Label "部署 frontend release" -Description "在本机启动前端静态站" -ScriptPath (Join-Path $toolsRoot "latest\deploy-docker.ps1") -InvocationName "latest deploy frontend" -DefaultArgs @("frontend") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接部署" -Description "按默认参数在本机启动 frontend")
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 deploy-docker.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-deploy-web" -Label "部署 web release" -Description "执行 web Docker 部署" -ScriptPath (Join-Path $toolsRoot "latest\deploy-docker.ps1") -InvocationName "latest deploy web" -DefaultArgs @("web") -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接部署" -Description "按默认参数部署 web")
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 deploy-docker.ps1 参数说明" -Args @("-Help"))
                ))
                (New-MenuCommand -Key "latest-installer" -Label "构建 workstation 安装器" -Description "执行 build-workstation-installer.ps1" -ScriptPath (Join-Path $toolsRoot "latest\build-workstation-installer.ps1") -InvocationName "latest installer" -Profiles @(
                    (New-MenuProfile -Key "default" -Label "直接构建" -Description "按默认参数构建安装器")
                    (New-MenuProfile -Key "noexe" -Label "只准备中间产物" -Description "跳过安装器 EXE 生成" -Args @("-NoExe"))
                    (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 build-workstation-installer.ps1 参数说明" -Args @("-Help"))
                ))
            )
        }
    )
}

function Invoke-InteractiveMenu {
    $groups = Get-MenuGroups

    while ($true) {
        $selectedGroup = Read-MenuChoice -Title "主菜单" -Options $groups -AllowQuit
        if ($selectedGroup -eq "__quit__") {
            return
        }

        while ($true) {
            $selectedCommand = Read-MenuChoice -Title ("{0} 菜单" -f $selectedGroup.Label) -Options $selectedGroup.Items -AllowBack -AllowQuit
            if ($selectedCommand -eq "__quit__") {
                return
            }
            if ($selectedCommand -eq "__back__") {
                break
            }

            $profiles = if ($selectedCommand.Profiles.Count -gt 0) { $selectedCommand.Profiles } else { @((New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认参数执行")) }
            $selectedProfile = Read-MenuChoice -Title ("参数模板 - {0}" -f $selectedCommand.Label) -Options $profiles -AllowBack -AllowQuit
            if ($selectedProfile -eq "__quit__") {
                return
            }
            if ($selectedProfile -eq "__back__") {
                continue
            }

            Confirm-And-InvokeMenuCommand -Command $selectedCommand -Profile $selectedProfile
        }
    }
}

function Invoke-Route {
    param(
        [string]$ResolvedGroup,
        [string]$ResolvedAction,
        [string[]]$ResolvedArgs = @()
    )

    $toolsRoot = $PSScriptRoot
    $groupName = $ResolvedGroup.ToLowerInvariant()
    $actionName = $ResolvedAction.ToLowerInvariant()

    switch ($groupName) {
        "install" {
            $installPs1 = Join-Path $toolsRoot "install\install.ps1"
            if ([string]::IsNullOrWhiteSpace($actionName)) {
                Invoke-TargetScript -Path $installPs1 -Arguments $ResolvedArgs
                return
            }
            if ($actionName -eq "mod") {
                Invoke-TargetScript -Path $installPs1 -Arguments (@("-OnlyModDeps") + $ResolvedArgs)
                return
            }
            if ($actionName -in @("help", "-h", "--help")) {
                Invoke-TargetScript -Path $installPs1 -Arguments @("-Help")
                return
            }
            Invoke-TargetScript -Path $installPs1 -Arguments (@($ResolvedAction) + $ResolvedArgs)
            return
        }
        "start" {
            switch ($actionName) {
                "workstation" { Invoke-TargetScript -Path (Join-Path $toolsRoot "start\start_workstation.bat") -Arguments $ResolvedArgs; return }
                "web" { Invoke-TargetScript -Path (Join-Path $toolsRoot "start\start_web.bat") -Arguments $ResolvedArgs; return }
                "dev" { Invoke-TargetScript -Path (Join-Path $toolsRoot "start\start_dev.bat") -Arguments $ResolvedArgs; return }
                default { Show-Help; return }
            }
        }
        "split" {
            switch ($actionName) {
                "start" { Invoke-TargetScript -Path (Join-Path $toolsRoot "split-local\start_split_local.ps1") -Arguments $ResolvedArgs; return }
                "stop" { Invoke-TargetScript -Path (Join-Path $toolsRoot "split-local\stop_split_local.ps1") -Arguments $ResolvedArgs; return }
                default { Show-Help; return }
            }
        }
        "dev" {
            switch ($actionName) {
                "decompile" { Invoke-TargetScript -Path (Join-Path $toolsRoot "dev\decompile_sts2.py") -Arguments $ResolvedArgs; return }
                default { Show-Help; return }
            }
        }
        "latest" {
            switch ($actionName) {
                "package" { Invoke-TargetScript -Path (Join-Path $toolsRoot "latest\package-release.ps1") -Arguments $ResolvedArgs; return }
                "deploy" { Invoke-TargetScript -Path (Join-Path $toolsRoot "latest\deploy-docker.ps1") -Arguments $ResolvedArgs; return }
                "installer" { Invoke-TargetScript -Path (Join-Path $toolsRoot "latest\build-workstation-installer.ps1") -Arguments $ResolvedArgs; return }
                default { Show-Help; return }
            }
        }
        "help" { Show-Help; return }
        default { Show-Help; return }
    }
}

if ($Help -and [string]::IsNullOrWhiteSpace($Group)) {
    Show-Help
    return
}

if ($Help -and -not [string]::IsNullOrWhiteSpace($Group)) {
    $RemainingArgs = @("-Help") + $RemainingArgs
}

if ([string]::IsNullOrWhiteSpace($Group)) {
    if (Test-InteractiveConsole) {
        Invoke-InteractiveMenu
    } else {
        Show-Help
    }
    return
}

Invoke-Route -ResolvedGroup $Group -ResolvedAction $Action -ResolvedArgs $RemainingArgs
