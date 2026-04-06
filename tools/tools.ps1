<#
.SYNOPSIS
统一的 tools 目录入口。

.DESCRIPTION
默认显示可选脚本目录；也支持通过参数直接路由到安装、启动、拆分运行时、开发辅助和 latest 打包部署脚本。

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
#>
param(
    [Parameter(Position = 0)]
    [string]$Group = "",

    [Parameter(Position = 1)]
    [string]$Action = "",

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

function Show-Help {
    Write-Host ""
    Write-Host "AgentTheSpire tools 统一入口" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "可用命令："
    Write-Host "  install                安装完整运行依赖"
    Write-Host "  install mod            只安装 Mod 开发依赖"
    Write-Host "  start full             启动兼容态 full"
    Write-Host "  start workstation      启动 workstation-backend"
    Write-Host "  start web              启动 web-backend"
    Write-Host "  start dev              启动开发模式"
    Write-Host "  split start            启动独立前端 + 本地 workstation"
    Write-Host "  split stop             停止 split-local 双进程"
    Write-Host "  dev verify-install     校验安装 wrapper"
    Write-Host "  dev decompile          反编译 sts2.dll"
    Write-Host "  latest package <target> 打包 release bundle"
    Write-Host "  latest deploy <target>  部署 Docker release"
    Write-Host "  latest installer       构建 workstation 安装器"
    Write-Host ""
    Write-Host "示例："
    Write-Host "  powershell -File .\tools\tools.ps1 install"
    Write-Host "  powershell -File .\tools\tools.ps1 install mod"
    Write-Host "  powershell -File .\tools\tools.ps1 start workstation"
    Write-Host "  powershell -File .\tools\tools.ps1 split start -DryRun"
    Write-Host "  powershell -File .\tools\tools.ps1 latest package workstation"
    Write-Host ""
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
            $powershellCommand = Get-Command powershell -ErrorAction SilentlyContinue
            if ($null -eq $powershellCommand) {
                throw "未找到 powershell，无法执行：$Path"
            }
            & $powershellCommand.Source -NoProfile -ExecutionPolicy Bypass -File $Path @Arguments
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

function Invoke-InteractiveMenu {
    Show-Help
    $commandLine = Read-Host "请输入命令（例如 install mod / start workstation / latest package workstation，直接回车退出）"
    if ([string]::IsNullOrWhiteSpace($commandLine)) {
        return
    }

    $tokens = @($commandLine -split '\s+' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($tokens.Count -eq 0) {
        return
    }

    $interactiveGroup = $tokens[0]
    $interactiveAction = if ($tokens.Count -ge 2) { $tokens[1] } else { "" }
    $interactiveRest = if ($tokens.Count -ge 3) { @($tokens[2..($tokens.Count - 1)]) } else { @() }
    Invoke-Route -ResolvedGroup $interactiveGroup -ResolvedAction $interactiveAction -ResolvedArgs $interactiveRest
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
                "" { Show-Help; return }
                "full" { Invoke-TargetScript -Path (Join-Path $toolsRoot "start\start.bat") -Arguments $ResolvedArgs; return }
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
                "verify-install" { Invoke-TargetScript -Path (Join-Path $toolsRoot "dev\verify-install-bat.ps1") -Arguments $ResolvedArgs; return }
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
        "-h" { Show-Help; return }
        "--help" { Show-Help; return }
        default { Show-Help; return }
    }
}

if ([string]::IsNullOrWhiteSpace($Group)) {
    Invoke-InteractiveMenu
    return
}

Invoke-Route -ResolvedGroup $Group -ResolvedAction $Action -ResolvedArgs $RemainingArgs
