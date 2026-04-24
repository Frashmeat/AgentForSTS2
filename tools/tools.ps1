<#
.SYNOPSIS
统一的 tools 目录入口。

.DESCRIPTION
支持两种使用方式：
1. 直接运行脚本，进入分层数字菜单，使用键盘选择要执行的脚本和参数模板
2. 通过参数直达具体脚本，例如 install / start / split / stop / dev / latest

.EXAMPLE
powershell -File .\tools\tools.ps1

.EXAMPLE
powershell -File .\tools\tools.ps1 install mod

.EXAMPLE
powershell -File .\tools\tools.ps1 start workstation

.EXAMPLE
powershell -File .\tools\tools.ps1 stop local

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

function Get-CurrentPowerShellExecutablePath {
    try {
        $currentProcessPath = (Get-Process -Id $PID -ErrorAction Stop).Path
        $leafName = [System.IO.Path]::GetFileNameWithoutExtension($currentProcessPath)
        if ($leafName -in @("powershell", "pwsh")) {
            return $currentProcessPath
        }
    }
    catch {
    }

    $preferredCommands = if ($PSVersionTable.PSEdition -eq "Core") {
        @("pwsh", "powershell")
    } else {
        @("powershell", "pwsh")
    }

    foreach ($commandName in $preferredCommands) {
        $command = Get-Command $commandName -ErrorAction SilentlyContinue
        if ($null -ne $command) {
            return $command.Source
        }
    }

    throw "未找到可用的 PowerShell 可执行文件。"
}

function Get-CurrentPowerShellCommandName {
    return [System.IO.Path]::GetFileNameWithoutExtension((Get-CurrentPowerShellExecutablePath))
}

function Get-PythonCommandPath {
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($null -eq $python) {
        throw "未找到 python 命令。"
    }

    return $python.Source
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
            & (Get-CurrentPowerShellExecutablePath) -NoProfile -ExecutionPolicy Bypass -File $Path @Arguments
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
            & (Get-PythonCommandPath) $Path @Arguments
            return
        }
        default {
            throw "不支持的脚本类型：$Path"
        }
    }
}

function Test-InteractiveConsole {
    try {
        return (-not [Console]::IsInputRedirected) -and (-not [Console]::IsOutputRedirected)
    }
    catch {
        return $true
    }
}

function New-MenuPrompt {
    param(
        [string]$Key,
        [string]$Prompt,
        [string]$ArgumentName = "",
        [ValidateSet("value", "switch")]
        [string]$Kind = "value",
        [switch]$Required
    )

    return [pscustomobject]@{
        Key = $Key
        Prompt = $Prompt
        ArgumentName = $ArgumentName
        Kind = $Kind
        Required = $Required.IsPresent
    }
}

function New-MenuProfile {
    param(
        [string]$Key,
        [string]$Label,
        [string]$Description,
        [string[]]$ProfileArgs = @(),
        [object[]]$Prompts = @()
    )

    return [pscustomobject]@{
        Key = $Key
        Label = $Label
        Description = $Description
        Arguments = $ProfileArgs
        Prompts = $Prompts
    }
}

function New-MenuCommand {
    param(
        [string]$Key,
        [string]$Action,
        [string]$Label,
        [string]$Description,
        [string]$ScriptPath,
        [string]$InvocationName,
        [string[]]$DefaultArgs = @(),
        [object[]]$Profiles = @(),
        [switch]$IsDefaultAction
    )

    return [pscustomobject]@{
        Key = $Key
        Action = $Action
        Label = $Label
        Description = $Description
        ScriptPath = $ScriptPath
        InvocationName = $InvocationName
        DefaultArgs = $DefaultArgs
        Profiles = $Profiles
        IsDefaultAction = $IsDefaultAction.IsPresent
    }
}

function New-MenuGroup {
    param(
        [string]$Key,
        [string]$Label,
        [string]$Description,
        [object[]]$Commands
    )

    return [pscustomobject]@{
        Key = $Key
        Label = $Label
        Description = $Description
        Commands = $Commands
    }
}

function Get-CommandCatalog {
    $toolsRoot = $PSScriptRoot

    return @(
        (New-MenuGroup -Key "install" -Label "安装" -Description "安装环境与运行依赖" -Commands @(
            (New-MenuCommand -Key "install-full" -Action "" -Label "完整安装" -Description "安装 .NET / Godot / 后端依赖 / 前端依赖 / 前端构建" -ScriptPath (Join-Path $toolsRoot "install\install.ps1") -InvocationName "install" -IsDefaultAction -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认流程完整安装")
                (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 install.ps1 参数说明" -ProfileArgs @("-Help"))
            ))
            (New-MenuCommand -Key "install-mod" -Action "mod" -Label "只安装 Mod 依赖" -Description "只安装 .NET 9 和 Godot 4.5.1 Mono" -ScriptPath (Join-Path $toolsRoot "install\install.ps1") -InvocationName "install mod" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "只安装或配置 Mod 开发依赖" -ProfileArgs @("-OnlyModDeps"))
                (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 install.ps1 参数说明" -ProfileArgs @("-Help"))
            ))
        ))
        (New-MenuGroup -Key "start" -Label "启动" -Description "启动 workstation / web / dev" -Commands @(
            (New-MenuCommand -Key "start-workstation" -Action "workstation" -Label "启动 workstation-backend" -Description "启动本地工作站后端" -ScriptPath (Join-Path $toolsRoot "start\start_workstation.bat") -InvocationName "start workstation" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认方式启动 workstation-backend")
            ))
            (New-MenuCommand -Key "start-web" -Action "web" -Label "启动 web-backend" -Description "启动平台 Web API 后端" -ScriptPath (Join-Path $toolsRoot "start\start_web.bat") -InvocationName "start web" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认方式启动 web-backend")
            ))
            (New-MenuCommand -Key "start-dev" -Action "dev" -Label "启动开发模式" -Description "拉起后端热重载和 Vite 前端开发服务器" -ScriptPath (Join-Path $toolsRoot "start\start_dev.bat") -InvocationName "start dev" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认方式启动开发模式")
            ))
        ))
        (New-MenuGroup -Key "split" -Label "拆分运行时" -Description "独立前端 + 本地 workstation 的双进程形态" -Commands @(
            (New-MenuCommand -Key "split-start" -Action "start" -Label "启动 split-local" -Description "启动独立前端 + 本地 workstation" -ScriptPath (Join-Path $toolsRoot "split-local\start_split_local.ps1") -InvocationName "split start" -Profiles @(
                (New-MenuProfile -Key "default" -Label "默认启动" -Description "使用默认端口直接启动")
                (New-MenuProfile -Key "dryrun" -Label "DryRun 预览" -Description "只打印布局和端口，不真正启动" -ProfileArgs @("-DryRun"))
                (New-MenuProfile -Key "nobrowser" -Label "启动但不打开浏览器" -Description "适合后台或远程环境" -ProfileArgs @("-NoBrowser"))
                (New-MenuProfile -Key "custom" -Label "自定义端口/地址" -Description "逐项输入自定义端口和 Web API 地址" -Prompts @(
                    (New-MenuPrompt -Key "workstation-port" -Prompt "Workstation 端口（直接回车使用默认 7860）" -ArgumentName "-WorkstationPort")
                    (New-MenuPrompt -Key "frontend-port" -Prompt "前端静态端口（直接回车使用默认 8080）" -ArgumentName "-FrontendPort")
                    (New-MenuPrompt -Key "web-base-url" -Prompt "Web API 基地址（直接回车使用默认 http://127.0.0.1:7870）" -ArgumentName "-WebBaseUrl")
                    (New-MenuPrompt -Key "no-browser" -Prompt "是否不自动打开浏览器？(y/N)" -ArgumentName "-NoBrowser" -Kind "switch")
                    (New-MenuPrompt -Key "dry-run" -Prompt "是否只做 DryRun 预览？(y/N)" -ArgumentName "-DryRun" -Kind "switch")
                ))
            ))
            (New-MenuCommand -Key "split-stop" -Action "stop" -Label "停止 split-local" -Description "停止 split-local 双进程并清理状态文件" -ScriptPath (Join-Path $toolsRoot "split-local\stop_split_local.ps1") -InvocationName "split stop" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "停止已启动的 split-local 进程")
            ))
        ))
        (New-MenuGroup -Key "stop" -Label "停止 / 清理" -Description "停止本机服务、清理本地状态" -Commands @(
            (New-MenuCommand -Key "stop-local" -Action "local" -Label "停止本机 frontend/workstation/web" -Description "停止当前仓库识别到的本地服务进程与默认 web compose" -ScriptPath (Join-Path $toolsRoot "kill-local.ps1") -InvocationName "stop local" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "按状态文件和默认端口停止本地服务")
                (New-MenuProfile -Key "custom" -Label "显式指定端口" -Description "按输入的端口覆盖默认发现逻辑" -Prompts @(
                    (New-MenuPrompt -Key "frontend-port" -Prompt "前端端口（直接回车跳过）" -ArgumentName "-FrontendPort")
                    (New-MenuPrompt -Key "workstation-port" -Prompt "Workstation 端口（直接回车跳过）" -ArgumentName "-WorkstationPort")
                    (New-MenuPrompt -Key "web-port" -Prompt "Web 端口（直接回车跳过）" -ArgumentName "-WebPort")
                ))
            ))
            (New-MenuCommand -Key "stop-deploy" -Action "deploy" -Label "停止 latest deploy 本地服务" -Description "读取 local-deploy-state.json 停止 release 本机进程" -ScriptPath (Join-Path $toolsRoot "latest\stop-deploy.ps1") -InvocationName "stop deploy" -DefaultArgs @("hybrid") -Profiles @(
                (New-MenuProfile -Key "hybrid" -Label "停止 hybrid" -Description "停止 hybrid release 拉起的本机进程" -ProfileArgs @("hybrid"))
                (New-MenuProfile -Key "workstation" -Label "停止 workstation" -Description "停止 workstation release 拉起的本机进程" -ProfileArgs @("workstation"))
                (New-MenuProfile -Key "frontend" -Label "停止 frontend" -Description "停止 frontend release 拉起的本机进程" -ProfileArgs @("frontend"))
                (New-MenuProfile -Key "web" -Label "停止 web" -Description "清理 web release 本地状态文件" -ProfileArgs @("web"))
                (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 stop-deploy.ps1 参数说明" -ProfileArgs @("-Help"))
            ))
        ))
        (New-MenuGroup -Key "dev" -Label "开发辅助" -Description "反编译游戏 DLL 等开发辅助" -Commands @(
            (New-MenuCommand -Key "dev-decompile" -Action "decompile" -Label "反编译 sts2.dll" -Description "运行 decompile_sts2.py" -ScriptPath (Join-Path $toolsRoot "dev\decompile_sts2.py") -InvocationName "dev decompile" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接执行" -Description "按当前配置运行反编译脚本")
            ))
        ))
        (New-MenuGroup -Key "latest" -Label "打包 / 部署" -Description "统一调度 tools/latest 下的发布脚本" -Commands @(
            (New-MenuCommand -Key "latest-package" -Action "package" -Label "打包 release" -Description "打包 release bundle（目标: hybrid / workstation / frontend / web）" -ScriptPath (Join-Path $toolsRoot "latest\package-release.ps1") -InvocationName "latest package" -Profiles @(
                (New-MenuProfile -Key "hybrid" -Label "打包 hybrid" -Description "构建正式推荐的 frontend + workstation 用户侧 bundle" -ProfileArgs @("hybrid"))
                (New-MenuProfile -Key "workstation" -Label "打包 workstation" -Description "构建前端 + workstation release bundle" -ProfileArgs @("workstation"))
                (New-MenuProfile -Key "frontend" -Label "打包 frontend" -Description "只打前端静态站点 release" -ProfileArgs @("frontend"))
                (New-MenuProfile -Key "web" -Label "打包 web" -Description "只打 Web API release" -ProfileArgs @("web"))
                (New-MenuProfile -Key "hybrid-nozip" -Label "打包 hybrid（不压缩）" -Description "保留 release 目录，跳过 zip" -ProfileArgs @("hybrid", "-NoZip"))
                (New-MenuProfile -Key "workstation-nozip" -Label "打包 workstation（不压缩）" -Description "保留 release 目录，跳过 zip" -ProfileArgs @("workstation", "-NoZip"))
                (New-MenuProfile -Key "frontend-nozip" -Label "打包 frontend（不压缩）" -Description "保留 release 目录，跳过 zip" -ProfileArgs @("frontend", "-NoZip"))
                (New-MenuProfile -Key "web-nozip" -Label "打包 web（不压缩）" -Description "保留 release 目录，跳过 zip" -ProfileArgs @("web", "-NoZip"))
                (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 package-release.ps1 参数说明" -ProfileArgs @("-Help"))
            ))
            (New-MenuCommand -Key "latest-deploy" -Action "deploy" -Label "部署 release" -Description "部署 release（目标: hybrid / workstation / frontend / web）" -ScriptPath (Join-Path $toolsRoot "latest\deploy-docker.ps1") -InvocationName "latest deploy" -Profiles @(
                (New-MenuProfile -Key "workstation" -Label "部署 workstation" -Description "按默认参数在本机启动 workstation" -ProfileArgs @("workstation"))
                (New-MenuProfile -Key "frontend" -Label "部署 frontend" -Description "按默认参数在本机启动 frontend" -ProfileArgs @("frontend"))
                (New-MenuProfile -Key "web" -Label "部署 web" -Description "按默认参数部署 web" -ProfileArgs @("web"))
                (New-MenuProfile -Key "hybrid-local-web" -Label "部署 hybrid（联动本机 Web）" -Description "显式使用 -DeployLocalWeb 联动部署本机 web-backend" -ProfileArgs @("hybrid", "-DeployLocalWeb"))
                (New-MenuProfile -Key "hybrid-remote-web" -Label "部署 hybrid（指定 Web API）" -Description "输入远端或本机 Web API 地址后部署 hybrid" -ProfileArgs @("hybrid") -Prompts @(
                    (New-MenuPrompt -Key "web-base-url" -Prompt "Web API 基地址（必填，例如 https://your-web-api.example.com）" -ArgumentName "-WebBaseUrl" -Required)
                ))
                (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 deploy-docker.ps1 参数说明" -ProfileArgs @("-Help"))
            ))
            (New-MenuCommand -Key "latest-installer" -Action "installer" -Label "构建 workstation 安装器" -Description "执行 build-workstation-installer.ps1" -ScriptPath (Join-Path $toolsRoot "latest\build-workstation-installer.ps1") -InvocationName "latest installer" -Profiles @(
                (New-MenuProfile -Key "default" -Label "直接构建" -Description "按默认参数构建安装器")
                (New-MenuProfile -Key "noexe" -Label "只准备中间产物" -Description "跳过安装器 EXE 生成" -ProfileArgs @("-NoExe"))
                (New-MenuProfile -Key "help" -Label "查看帮助" -Description "查看 build-workstation-installer.ps1 参数说明" -ProfileArgs @("-Help"))
            ))
        ))
    )
}

function Get-CommandUsage {
    param($Command)

    return [string]$Command.InvocationName
}

function Find-MenuCommand {
    param(
        [object[]]$Catalog,
        [string]$GroupKey,
        [string]$ActionKey
    )

    $group = $Catalog | Where-Object { $_.Key -eq $GroupKey } | Select-Object -First 1
    if ($null -eq $group) {
        return $null
    }

    if ([string]::IsNullOrWhiteSpace($ActionKey)) {
        return $group.Commands | Where-Object { $_.IsDefaultAction } | Select-Object -First 1
    }

    return $group.Commands | Where-Object { $_.Action -eq $ActionKey } | Select-Object -First 1
}

function Show-Help {
    param([object[]]$Catalog)

    $currentPowerShellName = Get-CurrentPowerShellCommandName

    Write-Host ""
    Write-Host "AgentTheSpire tools 统一入口" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "直接运行："
    Write-Host ("  {0} -File .\tools\tools.ps1" -f $currentPowerShellName)
    Write-Host "  进入分层数字菜单，可直接用键盘选择功能和参数模板。"
    Write-Host ""
    Write-Host "参数直达："

    foreach ($group in $Catalog) {
        foreach ($command in $group.Commands) {
            $usage = Get-CommandUsage -Command $command
            Write-Host ("  {0,-24} {1}" -f $usage, $command.Description)
        }
    }

    Write-Host ""
    Write-Host "常用示例："
    foreach ($example in @(
        "install",
        "install mod",
        "start workstation",
        "split start -DryRun",
        "stop local",
        "stop deploy hybrid",
        "latest package hybrid",
        "latest deploy hybrid -DeployLocalWeb",
        "latest deploy hybrid -WebBaseUrl https://your-web-api.example.com"
    )) {
        Write-Host ("  {0} -File .\tools\tools.ps1 {1}" -f $currentPowerShellName, $example)
    }
    Write-Host ""
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

function Resolve-ProfileArgs {
    param($Profile)

    $resolvedArgs = @($Profile.Arguments)
    foreach ($prompt in $Profile.Prompts) {
        while ($true) {
            $rawValue = Read-Host $prompt.Prompt

            switch ($prompt.Kind) {
                "switch" {
                    if ($rawValue -match '^(?i:y|yes)$') {
                        $resolvedArgs += $prompt.ArgumentName
                    }
                    break
                }
                "value" {
                    if ([string]::IsNullOrWhiteSpace($rawValue)) {
                        if ($prompt.Required) {
                            Write-Host "该项不能为空，请重新输入。" -ForegroundColor Yellow
                            continue
                        }
                        break
                    }

                    $resolvedArgs += @($prompt.ArgumentName, $rawValue.Trim())
                    break
                }
                default {
                    throw "未支持的 Prompt 类型：$($prompt.Kind)"
                }
            }

            break
        }
    }

    return $resolvedArgs
}

function Confirm-And-InvokeMenuCommand {
    param(
        $Command,
        $Profile
    )

    $resolvedArgs = @($Command.DefaultArgs + (Resolve-ProfileArgs -Profile $Profile))
    $previewArgs = if ($resolvedArgs.Count -gt 0) { $resolvedArgs -join " " } else { "(无参数)" }
    $currentPowerShellName = Get-CurrentPowerShellCommandName

    Write-Host ""
    Write-Host "将执行：" -ForegroundColor Cyan
    Write-Host ("  功能       : {0}" -f $Command.Label)
    Write-Host ("  参数模板   : {0}" -f $Profile.Label)
    Write-Host ("  脚本路径   : {0}" -f $Command.ScriptPath)
    Write-Host ("  命令预览   : {0} -File .\tools\tools.ps1 {1}" -f $currentPowerShellName, $Command.InvocationName)
    Write-Host ("  实际参数   : {0}" -f $previewArgs)
    Write-Host ""

    $confirm = Read-Host "确认执行？(Y/N)"
    if ($confirm -notmatch '^(?i:y|yes)$') {
        Write-Host "已取消执行。" -ForegroundColor Yellow
        return
    }

    Invoke-TargetScript -Path $Command.ScriptPath -Arguments $resolvedArgs
}

function Invoke-InteractiveMenu {
    param([object[]]$Catalog)

    while ($true) {
        $selectedGroup = Read-MenuChoice -Title "主菜单" -Options $Catalog -AllowQuit
        if ($selectedGroup -eq "__quit__") {
            return
        }

        while ($true) {
            $selectedCommand = Read-MenuChoice -Title ("{0} 菜单" -f $selectedGroup.Label) -Options $selectedGroup.Commands -AllowBack -AllowQuit
            if ($selectedCommand -eq "__quit__") {
                return
            }
            if ($selectedCommand -eq "__back__") {
                break
            }

            $profiles = if ($selectedCommand.Profiles.Count -gt 0) {
                $selectedCommand.Profiles
            } else {
                @((New-MenuProfile -Key "default" -Label "直接执行" -Description "按默认参数执行"))
            }

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
        [object[]]$Catalog,
        [string]$ResolvedGroup,
        [string]$ResolvedAction,
        [string[]]$ResolvedArgs = @()
    )

    $groupName = $ResolvedGroup.ToLowerInvariant()
    $actionName = $ResolvedAction.ToLowerInvariant()

    if ($groupName -eq "help") {
        Show-Help -Catalog $Catalog
        return
    }

    $command = Find-MenuCommand -Catalog $Catalog -GroupKey $groupName -ActionKey $actionName
    if ($null -eq $command) {
        Show-Help -Catalog $Catalog
        return
    }

    $effectiveArgs = @($ResolvedArgs)
    if ($effectiveArgs.Count -eq 0 -and @($command.DefaultArgs).Count -gt 0) {
        $effectiveArgs = @($command.DefaultArgs)
    }

    Invoke-TargetScript -Path $command.ScriptPath -Arguments $effectiveArgs
}

$commandCatalog = Get-CommandCatalog

if ($Help -and [string]::IsNullOrWhiteSpace($Group)) {
    Show-Help -Catalog $commandCatalog
    return
}

if ($Help -and -not [string]::IsNullOrWhiteSpace($Group)) {
    $RemainingArgs = @("-Help") + $RemainingArgs
}

$RemainingArgs = @(
    $RemainingArgs | Where-Object {
        -not [string]::IsNullOrWhiteSpace($_)
    }
)

if ([string]::IsNullOrWhiteSpace($Group)) {
    if (Test-InteractiveConsole) {
        Invoke-InteractiveMenu -Catalog $commandCatalog
    } else {
        Show-Help -Catalog $commandCatalog
    }
    return
}

Invoke-Route -Catalog $commandCatalog -ResolvedGroup $Group -ResolvedAction $Action -ResolvedArgs $RemainingArgs
