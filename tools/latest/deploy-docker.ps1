<#
.SYNOPSIS
按目标启动 AgentTheSpire 的混合部署。

.DESCRIPTION
读取 release bundle、生成运行时配置，并按目标在本机或 Docker 中启动对应服务。
直接执行脚本且不传任何参数时，会默认显示本帮助而不是立即启动部署。

.PARAMETER Target
部署目标。可选 hybrid / workstation / frontend / web。

.PARAMETER ReleaseRoot
release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。

.PARAMETER ConfigPath
输入配置文件路径。未显式传入时优先使用 release 内 `runtime/*.config.json`，再回退到服务目录内 `config.example.json`。

.PARAMETER ProjectName
Compose 项目名。默认按 agentthespire-<target>-release 生成。

.PARAMETER WorkstationPort
工作站端口。默认 7860。

.PARAMETER WebPort
Web 端口。默认 7870。

.PARAMETER FrontendPort
前端静态站端口。默认 8080。

.PARAMETER WebBaseUrl
前端运行时写入的 Web API 基地址。`frontend` 未显式传入时默认使用本机 `http://127.0.0.1:<WebPort>`；`hybrid` 目标必须显式传入，除非改用 `-DeployLocalWeb` 联动部署本机 Docker `web-backend`。

.PARAMETER WebReleaseRoot
`hybrid` 联动部署本机 `web-backend` 时使用的 release 目录。留空时默认按当前 hybrid release 的同级目录推导。

.PARAMETER DeployLocalWeb
仅用于 `hybrid`。显式要求联动部署本机 Docker `web-backend`，并把前端 `web` 地址写为 `http://127.0.0.1:<WebPort>`。

.PARAMETER PostgresHostPort
Postgres 暴露到宿主机的端口。默认 5432。

.PARAMETER PostgresDb
Postgres 数据库名。默认 agentthespire。

.PARAMETER PostgresUser
Postgres 用户名。默认 agentthespire。

.PARAMETER PostgresPassword
Postgres 密码。默认 agentthespire。

.PARAMETER PostgresImage
Postgres 镜像名。留空时自动优先复用本机已有镜像。

.PARAMETER PythonBaseImage
Python Docker 基础镜像。留空时自动优先复用本机已有标签，并回退到可用镜像源。

.PARAMETER ResetDatabase
重建数据库。仅适用于 web 目标。

.PARAMETER ReuseImages
复用已有镜像。仅在镜像缺失时才执行 docker compose build。

.PARAMETER RebuildImages
强制重建镜像。会删除当前项目对应镜像并重新 docker compose build。

.PARAMETER Help
显示帮助说明并退出。

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 workstation

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 hybrid -WebBaseUrl https://your-web-api.example.com

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 web -ResetDb -dbn agentthespire

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 hybrid -DeployLocalWeb
#>
[CmdletBinding()]
param(
    # 基础参数
    [Parameter(Position = 0, HelpMessage = "部署目标。可选 hybrid / workstation / frontend / web。")]
    [Alias("t")]
    [ValidateSet("hybrid", "workstation", "frontend", "web")]
    [string]$Target = "workstation",

    [Parameter(HelpMessage = "release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。")]
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Parameter(HelpMessage = "输入配置文件路径。未显式传入时优先使用 release 内 runtime 配置，再回退到服务目录内 config.example.json。")]
    [Alias("c")]
    [string]$ConfigPath = "",

    [Parameter(HelpMessage = "Compose 项目名。默认按 agentthespire-<target>-release 生成。")]
    [Alias("n")]
    [string]$ProjectName = "",

    # 端口参数
    [Parameter(HelpMessage = "工作站端口。默认 7860。")]
    [Alias("ws")]
    [string]$WorkstationPort = "7860",

    [Parameter(HelpMessage = "Web 端口。默认 7870。")]
    [Alias("wp")]
    [string]$WebPort = "7870",

    [Parameter(HelpMessage = "前端静态站端口。默认 8080。")]
    [Alias("fp")]
    [string]$FrontendPort = "8080",

    [Parameter(HelpMessage = "前端运行时写入的 Web API 基地址。`frontend` 未显式传入时默认使用本机 http://127.0.0.1:<WebPort>；`hybrid` 必须显式传入，除非改用 -DeployLocalWeb 联动本机 web-backend。")]
    [Alias("wb")]
    [string]$WebBaseUrl = "",

    [Parameter(HelpMessage = "`hybrid` 联动部署本机 web-backend 时使用的本机 web release 目录。留空时按当前 hybrid release 的同级目录推导。")]
    [string]$WebReleaseRoot = "",

    [Parameter(HelpMessage = "仅用于 hybrid。显式联动部署本机 web-backend，并把前端 Web API 基址写为本机 http://127.0.0.1:<WebPort>。")]
    [switch]$DeployLocalWeb,

    # 数据库参数
    [Parameter(HelpMessage = "Postgres 暴露到宿主机的端口。默认 5432。")]
    [Alias("dbp")]
    [string]$PostgresHostPort = "5432",

    [Parameter(HelpMessage = "Postgres 数据库名。默认 agentthespire。")]
    [Alias("dbn")]
    [string]$PostgresDb = "agentthespire",

    [Parameter(HelpMessage = "Postgres 用户名。默认 agentthespire。")]
    [Alias("dbu")]
    [string]$PostgresUser = "agentthespire",

    [Parameter(HelpMessage = "Postgres 密码。默认 agentthespire。")]
    [Alias("dbpw")]
    [string]$PostgresPassword = "agentthespire",

    [Parameter(HelpMessage = "Postgres 镜像名。留空时自动优先复用本机已有镜像。")]
    [Alias("pg")]
    [string]$PostgresImage = "",

    [Parameter(HelpMessage = "Python Docker 基础镜像。留空时自动优先复用本机已有标签，并回退到可用镜像源。")]
    [Alias("pyi")]
    [string]$PythonBaseImage = "",

    # 行为开关
    [Parameter(HelpMessage = "重建数据库。仅适用于 web 目标。")]
    [Alias("ResetDb")]
    [switch]$ResetDatabase,

    [Parameter(HelpMessage = "复用已有镜像。仅在镜像缺失时才执行 docker compose build。")]
    [Alias("Reuse")]
    [switch]$ReuseImages,

    [Parameter(HelpMessage = "强制重建镜像。会删除当前项目对应镜像并重新 docker compose build。")]
    [Alias("Rebuild")]
    [switch]$RebuildImages,

    [Parameter(HelpMessage = "显示帮助说明并退出。")]
    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help -or $PSBoundParameters.Count -eq 0) {
    Get-Help -Full $PSCommandPath | Out-String | Write-Output
    return
}

if ($ReuseImages.IsPresent -and $RebuildImages.IsPresent) {
    throw "-ReuseImages 与 -RebuildImages 不能同时使用。"
}

if ($DeployLocalWeb.IsPresent -and $Target -ne "hybrid") {
    throw "-DeployLocalWeb 仅适用于 hybrid 目标。"
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

function Resolve-PythonCommand {
    param(
        [string]$BackendRoot = "",
        [string]$RepoRoot = ""
    )

    $explicitPython = [Environment]::GetEnvironmentVariable("ATS_PYTHON_COMMAND")
    if (-not [string]::IsNullOrWhiteSpace($explicitPython)) {
        return $explicitPython
    }

    if (-not [string]::IsNullOrWhiteSpace($BackendRoot)) {
        $venvPython = Join-Path $BackendRoot ".venv\Scripts\python.exe"
        if (Test-Path -LiteralPath $venvPython) {
            return $venvPython
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($RepoRoot)) {
        $repoVenvPython = Join-Path $RepoRoot "backend\.venv\Scripts\python.exe"
        if (Test-Path -LiteralPath $repoVenvPython) {
            return $repoVenvPython
        }
    }

    $globalPython = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $globalPython) {
        return $globalPython.Source
    }

    throw "未找到可用的 Python 解释器。"
}

function Test-PythonModulesAvailable {
    param(
        [string]$PythonExe,
        [string[]]$Modules
    )

    if (-not (Test-Path -LiteralPath $PythonExe)) {
        return $false
    }

    $script = @"
import importlib.util
import sys

missing = [name for name in sys.argv[1:] if importlib.util.find_spec(name) is None]
if missing:
    print(",".join(missing))
    raise SystemExit(1)
"@

    & $PythonExe -c $script @Modules 1>$null 2>$null
    return $LASTEXITCODE -eq 0
}

function Invoke-PipInstallWithFallback {
    param(
        [string]$PythonExe,
        [string]$WorkingDirectory,
        [string[]]$PipArguments
    )

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
        Write-Host "  pip 源        : $source"
        Push-Location $WorkingDirectory
        try {
            & $PythonExe -m pip install --disable-pip-version-check --default-timeout 60 --retries 2 --index-url $source @PipArguments | Out-Host
            $exitCode = $LASTEXITCODE
        }
        finally {
            Pop-Location
        }

        if ($exitCode -eq 0) {
            return
        }
    }

    throw "所有可用 pip 源都失败，请检查网络或先设置 PIP_INDEX_URL 后重试。"
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

function Get-LocalBackendRuntimeCacheRoot {
    param([string]$ReleaseRoot)

    return Join-Path (Join-Path (Join-Path $ReleaseRoot "runtime") "python-runtime") "workstation"
}

function Get-LocalBackendRuntimePythonPath {
    param([string]$ReleaseRoot)

    return Join-Path (Get-LocalBackendRuntimeCacheRoot -ReleaseRoot $ReleaseRoot) ".venv\Scripts\python.exe"
}

function Get-LocalBackendRuntimeMetadataPath {
    param([string]$ReleaseRoot)

    return Join-Path (Get-LocalBackendRuntimeCacheRoot -ReleaseRoot $ReleaseRoot) "cache-metadata.json"
}

function Get-FileSha256 {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return ""
    }

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

function Test-LocalBackendRuntimeCacheFresh {
    param(
        [string]$ReleaseRoot,
        [string]$RequirementsPath,
        [string]$BootstrapPython
    )

    $metadataPath = Get-LocalBackendRuntimeMetadataPath -ReleaseRoot $ReleaseRoot
    if (-not (Test-Path -LiteralPath $metadataPath)) {
        return $false
    }

    try {
        $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json -AsHashtable
    } catch {
        return $false
    }

    if ($null -eq $metadata) {
        return $false
    }

    $expectedRequirementsHash = Get-FileSha256 -Path $RequirementsPath
    $resolvedBootstrapPython = [System.IO.Path]::GetFullPath($BootstrapPython)
    return (
        [string]$metadata["requirements_sha256"] -eq $expectedRequirementsHash -and
        [string]$metadata["bootstrap_python"] -eq $resolvedBootstrapPython
    )
}

function Write-LocalBackendRuntimeMetadata {
    param(
        [string]$ReleaseRoot,
        [string]$RequirementsPath,
        [string]$BootstrapPython
    )

    $metadataPath = Get-LocalBackendRuntimeMetadataPath -ReleaseRoot $ReleaseRoot
    $metadataDir = Split-Path -Path $metadataPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($metadataDir)) {
        $null = New-Item -ItemType Directory -Path $metadataDir -Force
    }

    $payload = [ordered]@{
        requirements_sha256 = Get-FileSha256 -Path $RequirementsPath
        bootstrap_python = [System.IO.Path]::GetFullPath($BootstrapPython)
        updated_at = (Get-Date).ToString("o")
    }
    $payload | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $metadataPath -Encoding UTF8
}

function Ensure-LocalBackendRuntimePython {
    param(
        [string]$BackendRoot,
        [string]$RepoRoot,
        [string]$ReleaseRoot
    )

    $requiredModules = @("uvicorn", "fastapi", "sqlalchemy")
    $requirementsPath = Join-Path $BackendRoot "requirements.txt"
    if (-not (Test-Path -LiteralPath $requirementsPath)) {
        throw "缺少 backend/requirements.txt，无法准备本地 Python 运行时：$requirementsPath"
    }

    $bootstrapPython = Resolve-PythonCommand -RepoRoot $RepoRoot
    $runtimeCacheRoot = Get-LocalBackendRuntimeCacheRoot -ReleaseRoot $ReleaseRoot
    $runtimeVenvPython = Get-LocalBackendRuntimePythonPath -ReleaseRoot $ReleaseRoot
    if (
        (Test-PythonModulesAvailable -PythonExe $runtimeVenvPython -Modules $requiredModules) -and
        (Test-LocalBackendRuntimeCacheFresh -ReleaseRoot $ReleaseRoot -RequirementsPath $requirementsPath -BootstrapPython $bootstrapPython)
    ) {
        return $runtimeVenvPython
    }

    $serviceVenvPython = Join-Path $BackendRoot ".venv\Scripts\python.exe"
    if (Test-PythonModulesAvailable -PythonExe $serviceVenvPython -Modules $requiredModules) {
        return $serviceVenvPython
    }

    $repoVenvPython = Join-Path $RepoRoot "backend\.venv\Scripts\python.exe"
    if (Test-PythonModulesAvailable -PythonExe $repoVenvPython -Modules $requiredModules) {
        return $repoVenvPython
    }

    Write-Host "检测到本地 workstation Python 依赖未就绪，开始准备 release 运行时缓存..."
    Write-Host "  BackendRoot    : $BackendRoot"
    Write-Host "  RuntimeCache   : $runtimeCacheRoot"
    Write-Host "  Bootstrap Py   : $bootstrapPython"

    Remove-DirectoryIfExists -Path $runtimeCacheRoot
    $null = New-Item -ItemType Directory -Path $runtimeCacheRoot -Force

    & $bootstrapPython -m venv (Join-Path $runtimeCacheRoot ".venv") | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "创建 release runtime Python 缓存失败。"
    }

    Write-Host "  升级 pip..."
    Invoke-PipInstallWithFallback -PythonExe $runtimeVenvPython -WorkingDirectory $BackendRoot -PipArguments @("--upgrade", "pip")
    Write-Host "  安装 requirements.txt..."
    Invoke-PipInstallWithFallback -PythonExe $runtimeVenvPython -WorkingDirectory $BackendRoot -PipArguments @("-r", "requirements.txt")

    if (-not (Test-PythonModulesAvailable -PythonExe $runtimeVenvPython -Modules $requiredModules)) {
        throw "release runtime Python 缓存依赖安装后仍不完整，请检查 pip 输出。"
    }

    Write-LocalBackendRuntimeMetadata -ReleaseRoot $ReleaseRoot -RequirementsPath $requirementsPath -BootstrapPython $bootstrapPython
    return $runtimeVenvPython
}

function Get-ProcessLogPaths {
    param(
        [string]$ReleaseRoot,
        [string]$ServiceName
    )

    $logDir = Join-Path (Join-Path $ReleaseRoot "runtime") "logs"
    $null = New-Item -ItemType Directory -Path $logDir -Force
    return @{
        StdOut = Join-Path $logDir ("{0}.stdout.log" -f $ServiceName)
        StdErr = Join-Path $logDir ("{0}.stderr.log" -f $ServiceName)
    }
}

function Get-LocalLogMirrorRegistry {
    if (-not (Get-Variable -Name ATS_LOCAL_LOG_MIRRORS -Scope Global -ErrorAction SilentlyContinue)) {
        $global:ATS_LOCAL_LOG_MIRRORS = @{}
    }

    return $global:ATS_LOCAL_LOG_MIRRORS
}

function Stop-LocalLogMirroring {
    param([string]$ServiceName)

    $registry = Get-LocalLogMirrorRegistry
    if (-not $registry.ContainsKey($ServiceName)) {
        return
    }

    $entry = $registry[$ServiceName]
    foreach ($sourceIdentifier in @($entry.SourceIdentifiers)) {
        Unregister-Event -SourceIdentifier $sourceIdentifier -ErrorAction SilentlyContinue
    }

    foreach ($job in @($entry.Jobs)) {
        if ($null -ne $job) {
            Remove-Job -Id $job.Id -Force -ErrorAction SilentlyContinue
        }
    }

    foreach ($writer in @($entry.Writers)) {
        if ($null -ne $writer) {
            $writer.Dispose()
        }
    }

    $registry.Remove($ServiceName)
}

function New-ProcessStartInfo {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$WorkingDirectory
    )

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true

    if ($FilePath -match "\.(cmd|bat)$") {
        $startInfo.FileName = $env:ComSpec
        $startInfo.ArgumentList.Add("/d")
        $startInfo.ArgumentList.Add("/c")
        $startInfo.ArgumentList.Add($FilePath)
        foreach ($argument in $ArgumentList) {
            $startInfo.ArgumentList.Add([string]$argument)
        }
        return $startInfo
    }

    $startInfo.FileName = $FilePath
    foreach ($argument in $ArgumentList) {
        $startInfo.ArgumentList.Add([string]$argument)
    }
    return $startInfo
}

function New-LogWriter {
    param(
        [string]$PreferredPath,
        [string]$ServiceName,
        [string]$StreamName,
        [System.Text.Encoding]$Encoding
    )

    $parentDir = Split-Path -Path $PreferredPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($parentDir)) {
        $null = New-Item -ItemType Directory -Path $parentDir -Force
    }

    $fileNameWithoutExtension = [System.IO.Path]::GetFileNameWithoutExtension($PreferredPath)
    $extension = [System.IO.Path]::GetExtension($PreferredPath)
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $candidates = @($PreferredPath)
    for ($attempt = 1; $attempt -le 8; $attempt += 1) {
        $candidates += Join-Path $parentDir ("{0}.{1}-{2}{3}" -f $fileNameWithoutExtension, $timestamp, $attempt, $extension)
    }

    $lastError = ""
    foreach ($candidate in $candidates) {
        try {
            $stream = New-Object System.IO.FileStream(
                $candidate,
                [System.IO.FileMode]::Create,
                [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::ReadWrite
            )
            $writer = New-Object System.IO.StreamWriter($stream, $Encoding)
            $writer.AutoFlush = $true

            if ($candidate -ne $PreferredPath) {
                Write-Host ("日志文件被占用，{0}/{1} 改用: {2}" -f $ServiceName, $StreamName, $candidate) -ForegroundColor Yellow
            }

            return [pscustomobject]@{
                Path = $candidate
                Writer = $writer
            }
        } catch [System.IO.IOException], [System.UnauthorizedAccessException] {
            $lastError = $_.Exception.Message
        }
    }

    throw "无法为 $ServiceName/$StreamName 创建日志文件。首选路径: $PreferredPath。最后错误: $lastError"
}

function Start-LocalProcessWithMirroredLogs {
    param(
        [string]$ServiceName,
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$WorkingDirectory,
        [string]$StdOutPath,
        [string]$StdErrPath
    )

    Stop-LocalLogMirroring -ServiceName $ServiceName

    $stdoutDir = Split-Path -Path $StdOutPath -Parent
    $stderrDir = Split-Path -Path $StdErrPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($stdoutDir)) {
        $null = New-Item -ItemType Directory -Path $stdoutDir -Force
    }
    if (-not [string]::IsNullOrWhiteSpace($stderrDir)) {
        $null = New-Item -ItemType Directory -Path $stderrDir -Force
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $stdoutLog = New-LogWriter -PreferredPath $StdOutPath -ServiceName $ServiceName -StreamName "stdout" -Encoding $utf8NoBom
    $stderrLog = New-LogWriter -PreferredPath $StdErrPath -ServiceName $ServiceName -StreamName "stderr" -Encoding $utf8NoBom

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = New-ProcessStartInfo -FilePath $FilePath -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory
    $process.EnableRaisingEvents = $true

    $stdoutWriter = $stdoutLog.Writer
    $stderrWriter = $stderrLog.Writer

    if (-not $process.Start()) {
        $stdoutWriter.Dispose()
        $stderrWriter.Dispose()
        throw "启动本地进程失败：$ServiceName"
    }

    $stdoutSourceIdentifier = "ATS.LocalLogMirror.$ServiceName.$($process.Id).stdout"
    $stderrSourceIdentifier = "ATS.LocalLogMirror.$ServiceName.$($process.Id).stderr"
    $stdoutJob = Register-ObjectEvent -InputObject $process -EventName OutputDataReceived -SourceIdentifier $stdoutSourceIdentifier -MessageData @{
        Writer = $stdoutWriter
        ServiceName = $ServiceName
        StreamName = "stdout"
    } -Action {
        $line = $Event.SourceEventArgs.Data
        if ($null -eq $line) {
            return
        }

        $Event.MessageData.Writer.WriteLine($line)
    }
    $stderrJob = Register-ObjectEvent -InputObject $process -EventName ErrorDataReceived -SourceIdentifier $stderrSourceIdentifier -MessageData @{
        Writer = $stderrWriter
        ServiceName = $ServiceName
        StreamName = "stderr"
    } -Action {
        $line = $Event.SourceEventArgs.Data
        if ($null -eq $line) {
            return
        }

        $Event.MessageData.Writer.WriteLine($line)
    }

    $registry = Get-LocalLogMirrorRegistry
    $registry[$ServiceName] = @{
        SourceIdentifiers = @($stdoutSourceIdentifier, $stderrSourceIdentifier)
        Jobs = @($stdoutJob, $stderrJob)
        Writers = @($stdoutWriter, $stderrWriter)
    }

    $process.BeginOutputReadLine()
    $process.BeginErrorReadLine()
    return [pscustomobject]@{
        Process = $process
        StdOutPath = $stdoutLog.Path
        StdErrPath = $stderrLog.Path
    }
}

function Get-LogTail {
    param(
        [string]$Path,
        [int]$Tail = 40
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return ""
    }

    return (Get-Content -LiteralPath $Path -Tail $Tail -ErrorAction SilentlyContinue) -join [Environment]::NewLine
}

function Wait-LocalServiceReady {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$ServiceName,
        [string]$Url,
        [string]$StdOutPath,
        [string]$StdErrPath,
        [int]$MaxAttempts = 30
    )

    if ([Environment]::GetEnvironmentVariable("ATS_SKIP_LOCAL_READY_CHECK") -eq "1") {
        Wait-Process -Id $Process.Id -Timeout 2 -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 200
        return
    }

    for ($attempt = 0; $attempt -lt $MaxAttempts; $attempt += 1) {
        if ($Process.HasExited) {
            $stdoutTail = Get-LogTail -Path $StdOutPath
            $stderrTail = Get-LogTail -Path $StdErrPath
            $detail = @(
                "本地服务启动失败：$ServiceName"
                "进程已退出，退出码: $($Process.ExitCode)"
                "stdout: $StdOutPath"
                "stderr: $StdErrPath"
            )
            if (-not [string]::IsNullOrWhiteSpace($stderrTail)) {
                $detail += "stderr 最近输出:`n$stderrTail"
            } elseif (-not [string]::IsNullOrWhiteSpace($stdoutTail)) {
                $detail += "stdout 最近输出:`n$stdoutTail"
            }
            throw ($detail -join [Environment]::NewLine)
        }

        try {
            Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2 | Out-Null
            return
        } catch {
            Start-Sleep -Seconds 1
        }
    }

    throw "等待本地服务就绪超时：$ServiceName -> $Url`nstdout: $StdOutPath`nstderr: $StdErrPath"
}

function Stop-ProcessListeningOnPort {
    param([int]$Port)

    $pids = @(
        netstat -ano 2>$null |
            Select-String ":$Port\s+.*LISTENING" |
            ForEach-Object { ($_ -split "\s+")[-1] } |
            Where-Object { $_ -match "^\d+$" } |
            Select-Object -Unique
    )

    foreach ($listeningProcessId in $pids) {
        try {
            Stop-Process -Id ([int]$listeningProcessId) -Force -ErrorAction Stop
        } catch {
        }
    }
}

function Get-LocalDeploymentStatePath {
    param([string]$ReleaseRoot)

    return Join-Path (Join-Path $ReleaseRoot "runtime") "local-deploy-state.json"
}

function Clear-LocalDeploymentState {
    param([string]$ReleaseRoot)

    Remove-Item -LiteralPath (Get-LocalDeploymentStatePath -ReleaseRoot $ReleaseRoot) -Force -ErrorAction SilentlyContinue
}

function Write-LocalDeploymentState {
    param(
        [string]$ReleaseRoot,
        [string]$TargetName,
        [object[]]$ProcessEntries
    )

    $statePath = Get-LocalDeploymentStatePath -ReleaseRoot $ReleaseRoot
    $runtimeDir = Split-Path -Path $statePath -Parent
    if (-not [string]::IsNullOrWhiteSpace($runtimeDir)) {
        $null = New-Item -ItemType Directory -Path $runtimeDir -Force
    }

    $payload = [ordered]@{
        target = $TargetName
        release_root = [System.IO.Path]::GetFullPath($ReleaseRoot)
        updated_at = (Get-Date).ToString("o")
        processes = @(
            $ProcessEntries |
                Where-Object { $null -ne $_ -and $null -ne $_.Process } |
                ForEach-Object {
                    [ordered]@{
                        service_name = $_.ServiceName
                        pid = $_.Process.Id
                        port = $_.Port
                    }
                }
        )
    }

    $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $statePath -Encoding UTF8
}

function Stop-TrackedLocalProcesses {
    param(
        [string]$ReleaseRoot,
        [object[]]$ProcessEntries
    )

    foreach ($entry in @($ProcessEntries)) {
        if ($null -eq $entry) {
            continue
        }

        if (-not [string]::IsNullOrWhiteSpace($entry.ServiceName)) {
            Stop-LogViewerWindow -ReleaseRoot $ReleaseRoot -ServiceName $entry.ServiceName
        }

        if ($null -eq $entry.Process) {
            continue
        }

        try {
            if (-not $entry.Process.HasExited) {
                Stop-Process -Id $entry.Process.Id -Force -ErrorAction Stop
            }
        } catch {
        }
    }
}

function Wait-HttpReady {
    param(
        [string]$Url,
        [int]$MaxAttempts = 30
    )

    if ([Environment]::GetEnvironmentVariable("ATS_SKIP_LOCAL_READY_CHECK") -eq "1") {
        return
    }

    for ($attempt = 0; $attempt -lt $MaxAttempts; $attempt += 1) {
        try {
            Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2 | Out-Null
            return
        } catch {
            Start-Sleep -Seconds 1
        }
    }

    throw "等待本地服务就绪超时：$Url"
}

function Test-AbsoluteHttpUrl {
    param([string]$Value)

    $uri = $null
    if (-not [Uri]::TryCreate($Value, [UriKind]::Absolute, [ref]$uri)) {
        return $false
    }

    return $uri.Scheme -in @("http", "https")
}

function Get-CurrentPowerShellExecutablePath {
    return (Get-Process -Id $PID).Path
}

function Get-LogViewerPidPath {
    param(
        [string]$ReleaseRoot,
        [string]$ServiceName
    )

    return Join-Path (Join-Path $ReleaseRoot "runtime") ("{0}.log-viewer.pid" -f $ServiceName)
}

function Stop-LogViewerWindow {
    param(
        [string]$ReleaseRoot,
        [string]$ServiceName
    )

    $pidPath = Get-LogViewerPidPath -ReleaseRoot $ReleaseRoot -ServiceName $ServiceName
    if (-not (Test-Path -LiteralPath $pidPath)) {
        return
    }

    $viewerProcessId = ""
    try {
        $viewerProcessId = (Get-Content -LiteralPath $pidPath -Raw).Trim()
    } catch {
        $viewerProcessId = ""
    }

    if ($viewerProcessId -match "^\d+$") {
        try {
            Stop-Process -Id ([int]$viewerProcessId) -Force -ErrorAction Stop
        } catch {
        }
    }

    Remove-Item -LiteralPath $pidPath -Force -ErrorAction SilentlyContinue
}

function Ensure-LogViewerScript {
    param([string]$ReleaseRoot)

    $runtimeDir = Join-Path $ReleaseRoot "runtime"
    $null = New-Item -ItemType Directory -Path $runtimeDir -Force
    $scriptPath = Join-Path $runtimeDir "watch-service-logs.ps1"
    $content = @'
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ServiceName,

    [Parameter(Mandatory = $true)]
    [string]$StdOutPath,

    [Parameter(Mandatory = $true)]
    [string]$StdErrPath
)

$ErrorActionPreference = "Stop"

function Ensure-LogFile {
    param([string]$Path)

    if (Test-Path -LiteralPath $Path) {
        return
    }

    $parent = Split-Path -Path $Path -Parent
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        $null = New-Item -ItemType Directory -Path $parent -Force
    }
    $null = New-Item -ItemType File -Path $Path -Force
}

function Read-NewLogLines {
    param(
        [string]$Path,
        [string]$Label,
        [hashtable]$Offsets
    )

    Ensure-LogFile -Path $Path

    $offset = if ($Offsets.ContainsKey($Path)) { [int64]$Offsets[$Path] } else { 0L }
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $null = $stream.Seek($offset, [System.IO.SeekOrigin]::Begin)
        $reader = New-Object System.IO.StreamReader($stream)
        try {
            while (($line = $reader.ReadLine()) -ne $null) {
                Write-Host ("[{0}] {1}" -f $Label, $line)
            }
            $Offsets[$Path] = $stream.Position
        }
        finally {
            $reader.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

$Host.UI.RawUI.WindowTitle = "AgentTheSpire - $ServiceName 日志"
Write-Host "正在跟随 $ServiceName 日志..."
Write-Host "stdout: $StdOutPath"
Write-Host "stderr: $StdErrPath"
Write-Host ""

$offsets = @{}
while ($true) {
    Read-NewLogLines -Path $StdOutPath -Label "$ServiceName/stdout" -Offsets $offsets
    Read-NewLogLines -Path $StdErrPath -Label "$ServiceName/stderr" -Offsets $offsets
    Start-Sleep -Milliseconds 350
}
'@
    Set-Content -LiteralPath $scriptPath -Value $content -Encoding UTF8
    return $scriptPath
}

function Start-LogViewerWindow {
    param(
        [string]$ReleaseRoot,
        [string]$ServiceName,
        [string]$StdOutPath,
        [string]$StdErrPath
    )

    $recordPath = [Environment]::GetEnvironmentVariable("ATS_LOG_VIEWER_RECORD_FILE")
    if (-not [string]::IsNullOrWhiteSpace($recordPath)) {
        Add-Content -LiteralPath $recordPath -Value ("{0}|{1}|{2}" -f $ServiceName, $StdOutPath, $StdErrPath) -Encoding UTF8
        return
    }

    if ([Environment]::GetEnvironmentVariable("ATS_DISABLE_LOG_WINDOWS") -eq "1") {
        return
    }

    Stop-LogViewerWindow -ReleaseRoot $ReleaseRoot -ServiceName $ServiceName

    $shellPath = Get-CurrentPowerShellExecutablePath
    $viewerScript = Ensure-LogViewerScript -ReleaseRoot $ReleaseRoot
    $viewerProcess = Start-Process -FilePath $shellPath -ArgumentList @(
        "-NoExit",
        "-File",
        $viewerScript,
        "-ServiceName",
        $ServiceName,
        "-StdOutPath",
        $StdOutPath,
        "-StdErrPath",
        $StdErrPath
    ) -PassThru

    Set-Content -LiteralPath (Get-LogViewerPidPath -ReleaseRoot $ReleaseRoot -ServiceName $ServiceName) -Value $viewerProcess.Id -Encoding UTF8
}

function Get-DefaultHybridWebReleaseRoot {
    param(
        [string]$HybridReleaseRoot,
        [string]$ExplicitWebReleaseRoot = ""
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitWebReleaseRoot)) {
        return [System.IO.Path]::GetFullPath($ExplicitWebReleaseRoot)
    }

    if (-not [string]::IsNullOrWhiteSpace($HybridReleaseRoot)) {
        $resolvedHybridRoot = [System.IO.Path]::GetFullPath($HybridReleaseRoot)
        $parentDir = Split-Path -Path $resolvedHybridRoot -Parent
        $leafName = Split-Path -Path $resolvedHybridRoot -Leaf
        if ($leafName -match "hybrid-release$") {
            return Join-Path $parentDir ($leafName -replace "hybrid-release$", "web-release")
        }
        return Join-Path $parentDir "agentthespire-web-release"
    }

    return Join-Path $PSScriptRoot "artifacts\agentthespire-web-release"
}

function Get-DefaultHybridWebProjectName {
    param([string]$HybridProjectName)

    if (-not [string]::IsNullOrWhiteSpace($HybridProjectName)) {
        if ($HybridProjectName -match "hybrid-release$") {
            return ($HybridProjectName -replace "hybrid-release$", "web-release")
        }
    }

    return "agentthespire-web-release"
}

function Ensure-HybridLocalWebRelease {
    param(
        [string]$WebReleaseRoot,
        [bool]$ForceRefresh = $false
    )

    if ((-not $ForceRefresh) -and (Test-Path -LiteralPath $WebReleaseRoot)) {
        return
    }

    $shellPath = Get-CurrentPowerShellExecutablePath
    $packageScriptPath = Join-Path $PSScriptRoot "package-release.ps1"
    $outputRoot = Split-Path -Path $WebReleaseRoot -Parent
    $releaseName = Split-Path -Path $WebReleaseRoot -Leaf

    if ($ForceRefresh) {
        Write-Host "刷新默认本机 web release，使其与当前仓库模板保持一致: $WebReleaseRoot"
    } else {
        Write-Host "未找到本机 web release，先自动生成: $WebReleaseRoot"
    }
    & $shellPath -NoProfile -File $packageScriptPath web -OutputRoot $outputRoot -ReleaseName $releaseName -SkipZip
    if ($LASTEXITCODE -ne 0) {
        throw "自动生成本机 web release 失败，退出码: $LASTEXITCODE"
    }
}

function Invoke-HybridLocalWebDeployment {
    param(
        [string]$HybridReleaseRoot,
        [string]$HybridProjectName,
        [string]$ExplicitWebReleaseRoot = ""
    )

    $webReleaseRoot = Get-DefaultHybridWebReleaseRoot -HybridReleaseRoot $HybridReleaseRoot -ExplicitWebReleaseRoot $ExplicitWebReleaseRoot
    $forceRefreshDefaultWebRelease = [string]::IsNullOrWhiteSpace($ExplicitWebReleaseRoot)
    if ($forceRefreshDefaultWebRelease) {
        Stop-ExistingComposeProject -BundleDir $webReleaseRoot -ComposeProjectName (Get-DefaultHybridWebProjectName -HybridProjectName $HybridProjectName)
    }
    Ensure-HybridLocalWebRelease -WebReleaseRoot $webReleaseRoot -ForceRefresh:$forceRefreshDefaultWebRelease
    Assert-PathExists -Path $webReleaseRoot -Label "hybrid 默认本机 web release 目录"

    $shellPath = Get-CurrentPowerShellExecutablePath
    $invokeArgs = @(
        "-NoProfile",
        "-File",
        $PSCommandPath,
        "web",
        "-ReleaseRoot",
        $webReleaseRoot,
        "-ProjectName",
        (Get-DefaultHybridWebProjectName -HybridProjectName $HybridProjectName),
        "-ConfigPath",
        $ConfigPath,
        "-WebPort",
        $WebPort,
        "-PostgresHostPort",
        $PostgresHostPort,
        "-PostgresDb",
        $PostgresDb,
        "-PostgresUser",
        $PostgresUser,
        "-PostgresPassword",
        $PostgresPassword
    )

    if (-not [string]::IsNullOrWhiteSpace($PostgresImage)) {
        $invokeArgs += @("-PostgresImage", $PostgresImage)
    }
    if (-not [string]::IsNullOrWhiteSpace($PythonBaseImage)) {
        $invokeArgs += @("-PythonBaseImage", $PythonBaseImage)
    }
    if ($ResetDatabase.IsPresent) {
        $invokeArgs += "-ResetDatabase"
    }
    if ($ReuseImages.IsPresent) {
        $invokeArgs += "-ReuseImages"
    }
    if ($RebuildImages.IsPresent) {
        $invokeArgs += "-RebuildImages"
    }

    Write-Host "hybrid 默认使用本机 Web API：先联动部署 web-backend ($webReleaseRoot)..."
    & $shellPath @invokeArgs
    if ($LASTEXITCODE -ne 0) {
        throw "hybrid 默认联动部署本机 web-backend 失败，退出码: $LASTEXITCODE"
    }
}

function Get-SourceConfigPath {
    param(
        [string]$PreferredPath,
        [string]$RuntimeConfigPath,
        [string]$FallbackServiceDir
    )

    if ((-not [string]::IsNullOrWhiteSpace($PreferredPath)) -and (Test-Path -LiteralPath $PreferredPath)) {
        return (Resolve-Path $PreferredPath).Path
    }

    if ((-not [string]::IsNullOrWhiteSpace($RuntimeConfigPath)) -and (Test-Path -LiteralPath $RuntimeConfigPath)) {
        return (Resolve-Path $RuntimeConfigPath).Path
    }

    $fallback = Join-Path $FallbackServiceDir "config.example.json"
    if (Test-Path -LiteralPath $fallback) {
        return $fallback
    }

    throw "未找到可用配置文件。请显式提供 -ConfigPath，或先准备 release 内的 runtime 配置文件。"
}

function Ensure-Hashtable {
    param([object]$Value)

    if ($Value -is [hashtable]) {
        return $Value
    }
    if ($null -eq $Value) {
        return @{}
    }
    return @{} + $Value
}

function New-RuntimeConfig {
    param(
        [string]$SourceConfigPath,
        [ValidateSet("workstation", "web")]
        [string]$Mode,
        [string]$DbUser,
        [string]$DbPassword,
        [string]$DbName
    )

    $config = Get-Content -LiteralPath $SourceConfigPath -Raw | ConvertFrom-Json -AsHashtable
    if (-not $config) {
        $config = @{}
    }

    $config["migration"] = Ensure-Hashtable -Value $config["migration"]
    $config["database"] = Ensure-Hashtable -Value $config["database"]

    if ($Mode -eq "web") {
        # Web 目标始终接管数据库连接，并强制打开平台 API 相关开关。
        $config["database"]["url"] = "postgresql+psycopg://{0}:{1}@postgres:5432/{2}" -f $DbUser, $DbPassword, $DbName
        $config["database"]["echo"] = $false
        $config["database"]["pool_pre_ping"] = $true
        $config["migration"]["platform_jobs_api_enabled"] = $true
        $config["migration"]["platform_service_split_enabled"] = $true
    } else {
        # workstation 中的工作站进程不应暴露 web 平台路由。
        $config["migration"]["platform_jobs_api_enabled"] = $false
        $config["migration"]["platform_service_split_enabled"] = $false
    }

    return $config
}

function Write-RuntimeConfigFile {
    param(
        [hashtable]$Config,
        [string]$OutputPath
    )

    $parentDir = Split-Path -Path $OutputPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($parentDir)) {
        $null = New-Item -ItemType Directory -Path $parentDir -Force
    }

    if (Test-Path -LiteralPath $OutputPath -PathType Container) {
        # runtime/*.config.json 属于脚本托管产物，若之前被错误创建成目录则在写入前自愈。
        Remove-Item -LiteralPath $OutputPath -Recurse -Force
    }

    $json = $Config | ConvertTo-Json -Depth 20
    Set-Content -LiteralPath $OutputPath -Value $json -Encoding UTF8
}

function Write-LocalServiceConfig {
    param(
        [hashtable]$Config,
        [string]$RuntimeMirrorPath
    )

    Write-RuntimeConfigFile -Config $Config -OutputPath $RuntimeMirrorPath
}

function Write-ComposeEnvFile {
    param(
        [string]$TargetName,
        [string]$EnvPath,
        [string]$ResolvedPostgresImage,
        [string]$ResolvedPythonBaseImage
    )

    # Compose 模板的端口、数据库和镜像选择都从 runtime/.env 注入，bundle 本身保持静态。
    $lines = switch ($TargetName) {
        "workstation" {
            @(
                "ATS_WORKSTATION_PORT=$WorkstationPort"
                "ATS_PYTHON_BASE_IMAGE=$ResolvedPythonBaseImage"
            )
        }
        "hybrid" {
            @(
                "ATS_WORKSTATION_PORT=$WorkstationPort"
                "ATS_FRONTEND_PORT=$FrontendPort"
            )
        }
        "frontend" {
            @("ATS_FRONTEND_PORT=$FrontendPort")
        }
        "web" {
            @(
                "ATS_WEB_PORT=$WebPort"
                "ATS_POSTGRES_HOST_PORT=$PostgresHostPort"
                "ATS_POSTGRES_DB=$PostgresDb"
                "ATS_POSTGRES_USER=$PostgresUser"
                "ATS_POSTGRES_PASSWORD=$PostgresPassword"
                "ATS_POSTGRES_IMAGE=$ResolvedPostgresImage"
                "ATS_PYTHON_BASE_IMAGE=$ResolvedPythonBaseImage"
            )
        }
        default {
            throw "未知 Target: $TargetName"
        }
    }

    Set-Content -LiteralPath $EnvPath -Value ($lines -join [Environment]::NewLine) -Encoding UTF8
}

function Invoke-DockerCompose {
    param(
        [string]$BundleDir,
        [string]$ComposeFile,
        [string]$EnvFile,
        [string]$ComposeProjectName,
        [string[]]$ComposeArgs,
        [string[]]$Services = @()
    )

    Push-Location $BundleDir
    try {
        $dockerArgs = @(
            "compose",
            "--project-name",
            $ComposeProjectName
        )
        if ((-not [string]::IsNullOrWhiteSpace($EnvFile)) -and (Test-Path -LiteralPath $EnvFile)) {
            $dockerArgs += @("--env-file", $EnvFile)
        }
        $dockerArgs += @("-f", $ComposeFile)
        $dockerArgs += $ComposeArgs
        $dockerArgs += $Services

        & docker @dockerArgs
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose 执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

function Stop-ExistingComposeProject {
    param(
        [string]$BundleDir,
        [string]$ComposeProjectName
    )

    if (-not (Test-Path -LiteralPath $BundleDir)) {
        return
    }

    $composeFile = Join-Path $BundleDir "docker-compose.yml"
    if (-not (Test-Path -LiteralPath $composeFile)) {
        return
    }

    Assert-CommandExists -CommandName "docker"
    $envFile = Join-Path (Join-Path $BundleDir "runtime") ".env"
    Write-Host "检测到将刷新现有 linked web release，先停止旧 Compose 项目: $ComposeProjectName"
    Invoke-DockerCompose -BundleDir $BundleDir -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $ComposeProjectName -ComposeArgs @("down", "--remove-orphans")
}

function Write-FrontendRuntimeConfig {
    param(
        [string]$OutputPath,
        [string]$ResolvedWorkstationBaseUrl,
        [string]$ResolvedWorkstationWsBaseUrl,
        [string]$ResolvedWebBaseUrl
    )

    $parentDir = Split-Path -Path $OutputPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($parentDir)) {
        $null = New-Item -ItemType Directory -Path $parentDir -Force
    }

    $content = @"
window.__AGENT_THE_SPIRE_API_BASES__ = {
  workstation: "$ResolvedWorkstationBaseUrl",
  web: "$ResolvedWebBaseUrl"
};

window.__AGENT_THE_SPIRE_WS_BASES__ = {
  workstation: "$ResolvedWorkstationWsBaseUrl"
};
"@
    Set-Content -LiteralPath $OutputPath -Value $content -Encoding UTF8
}

function Start-LocalWorkstationDeployment {
    param(
        [string]$ReleaseRoot,
        [string]$SourceConfigPath,
        [int]$ResolvedWorkstationPort,
        [string]$ResolvedWebBaseUrl,
        [string]$RepoRoot
    )

    $serviceRoot = Join-Path (Join-Path $ReleaseRoot "services") "workstation"
    $backendRoot = Join-Path $serviceRoot "backend"
    $frontendDist = Join-Path $serviceRoot "frontend\dist"
    $runtimeMirrorPath = Join-Path (Join-Path $ReleaseRoot "runtime") "workstation.config.json"
    $serviceConfigPath = Join-Path $serviceRoot "config.json"

    Assert-PathExists -Path $backendRoot -Label "workstation backend 目录"
    Assert-PathExists -Path $frontendDist -Label "workstation frontend/dist 目录"

    $workstationConfig = New-RuntimeConfig -SourceConfigPath $SourceConfigPath -Mode "workstation" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
    Write-LocalServiceConfig -Config $workstationConfig -RuntimeMirrorPath $runtimeMirrorPath
    Remove-FileIfExists -Path $serviceConfigPath
    Write-FrontendRuntimeConfig -OutputPath (Join-Path $frontendDist "runtime-config.js") `
        -ResolvedWorkstationBaseUrl ("http://127.0.0.1:{0}" -f $ResolvedWorkstationPort) `
        -ResolvedWorkstationWsBaseUrl ("ws://127.0.0.1:{0}" -f $ResolvedWorkstationPort) `
        -ResolvedWebBaseUrl $ResolvedWebBaseUrl

    $logPaths = Get-ProcessLogPaths -ReleaseRoot $ReleaseRoot -ServiceName "workstation"
    Stop-ProcessListeningOnPort -Port $ResolvedWorkstationPort
    $pythonCommand = if ([Environment]::GetEnvironmentVariable("ATS_SKIP_LOCAL_READY_CHECK") -eq "1") {
        Resolve-PythonCommand -BackendRoot $backendRoot -RepoRoot $RepoRoot
    } else {
        Ensure-LocalBackendRuntimePython -BackendRoot $backendRoot -RepoRoot $RepoRoot -ReleaseRoot $ReleaseRoot
    }
    Write-Host "启动本机 workstation-backend..."
    Write-Host "  Python 解释器 : $pythonCommand"
    Write-Host "  stdout 日志   : $($logPaths.StdOut)"
    Write-Host "  stderr 日志   : $($logPaths.StdErr)"
    $processEntry = Start-LocalProcessWithMirroredLogs -ServiceName "workstation" -FilePath $pythonCommand -ArgumentList @(
        "-m",
        "uvicorn",
        "main_workstation:app",
        "--host",
        "127.0.0.1",
        "--port",
        "$ResolvedWorkstationPort"
    ) -WorkingDirectory $backendRoot -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr
    Start-LogViewerWindow -ReleaseRoot $ReleaseRoot -ServiceName "workstation" -StdOutPath $processEntry.StdOutPath -StdErrPath $processEntry.StdErrPath

    try {
        Wait-LocalServiceReady -Process $processEntry.Process -ServiceName "workstation-backend" -Url ("http://127.0.0.1:{0}/api/config" -f $ResolvedWorkstationPort) -StdOutPath $processEntry.StdOutPath -StdErrPath $processEntry.StdErrPath
    } catch {
        try {
            Stop-Process -Id $processEntry.Process.Id -Force -ErrorAction Stop
        } catch {
        }
        throw
    }

    return $processEntry.Process
}

function Start-LocalFrontendDeployment {
    param(
        [string]$ReleaseRoot,
        [string]$FrontendDist,
        [int]$ResolvedFrontendPort,
        [int]$ResolvedWorkstationPort,
        [string]$ResolvedWebBaseUrl,
        [string]$RepoRoot
    )

    Assert-PathExists -Path $FrontendDist -Label "frontend/dist 目录"

    Write-FrontendRuntimeConfig -OutputPath (Join-Path $FrontendDist "runtime-config.js") `
        -ResolvedWorkstationBaseUrl ("http://127.0.0.1:{0}" -f $ResolvedWorkstationPort) `
        -ResolvedWorkstationWsBaseUrl ("ws://127.0.0.1:{0}" -f $ResolvedWorkstationPort) `
        -ResolvedWebBaseUrl $ResolvedWebBaseUrl

    $logPaths = Get-ProcessLogPaths -ReleaseRoot $ReleaseRoot -ServiceName "frontend"
    Stop-ProcessListeningOnPort -Port $ResolvedFrontendPort
    $pythonCommand = Resolve-PythonCommand -RepoRoot $RepoRoot
    Write-Host "启动本机 frontend 静态服务..."
    Write-Host "  Python 解释器 : $pythonCommand"
    Write-Host "  stdout 日志   : $($logPaths.StdOut)"
    Write-Host "  stderr 日志   : $($logPaths.StdErr)"
    $processEntry = Start-LocalProcessWithMirroredLogs -ServiceName "frontend" -FilePath $pythonCommand -ArgumentList @(
        "-m",
        "http.server",
        "$ResolvedFrontendPort",
        "--bind",
        "127.0.0.1",
        "--directory",
        $FrontendDist
    ) -WorkingDirectory $FrontendDist -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr
    Start-LogViewerWindow -ReleaseRoot $ReleaseRoot -ServiceName "frontend" -StdOutPath $processEntry.StdOutPath -StdErrPath $processEntry.StdErrPath

    try {
        Wait-LocalServiceReady -Process $processEntry.Process -ServiceName "frontend-static" -Url ("http://127.0.0.1:{0}/runtime-config.js" -f $ResolvedFrontendPort) -StdOutPath $processEntry.StdOutPath -StdErrPath $processEntry.StdErrPath
    } catch {
        try {
            Stop-Process -Id $processEntry.Process.Id -Force -ErrorAction Stop
        } catch {
        }
        throw
    }

    return $processEntry.Process
}

function Get-BuildServices {
    param(
        [ValidateSet("hybrid", "workstation", "frontend", "web")]
        [string]$TargetName
    )

    switch ($TargetName) {
        "hybrid" { return @() }
        "workstation" { return @() }
        "frontend" { return @() }
        "web" { return @("web") }
        default { throw "未知 Target: $TargetName" }
    }
}

function Get-ComposeImageName {
    param(
        [string]$ComposeProjectName,
        [string]$ServiceName
    )

    return "{0}-{1}:latest" -f $ComposeProjectName, $ServiceName
}

function Test-DockerImageExists {
    param([string]$ImageName)

    try {
        & docker image inspect $ImageName 1>$null 2>$null
        return $LASTEXITCODE -eq 0
    }
    catch {
        return $false
    }
}

function Remove-DockerImageIfExists {
    param([string]$ImageName)

    if (Test-DockerImageExists -ImageName $ImageName) {
        & docker image rm -f $ImageName
        if ($LASTEXITCODE -ne 0) {
            throw "删除旧镜像失败: $ImageName"
        }
    }
}

function Resolve-PostgresImage {
    param([string]$PreferredImage)

    if (-not [string]::IsNullOrWhiteSpace($PreferredImage)) {
        return $PreferredImage
    }

    # 优先复用本机已有镜像，减少首次以外的网络拉取；都不存在时再回退到默认 upstream 名称。
    $candidates = @(
        "postgres:16-alpine",
        "m.daocloud.io/docker.io/library/postgres:16-alpine",
        "postgres:15-alpine",
        "m.daocloud.io/docker.io/library/postgres:15-alpine"
    )

    foreach ($candidate in $candidates) {
        if (Test-DockerImageExists -ImageName $candidate) {
            return $candidate
        }
    }

    return "postgres:16-alpine"
}

function Resolve-PythonBaseImage {
    param([string]$PreferredImage)

    if (-not [string]::IsNullOrWhiteSpace($PreferredImage)) {
        return $PreferredImage
    }

    # 优先复用本机已有镜像，减少首次以外的网络拉取；都不存在时再回退到默认镜像源。
    $candidates = @(
        "python:3.11-slim",
        "m.daocloud.io/docker.io/library/python:3.11-slim"
    )

    foreach ($candidate in $candidates) {
        if (Test-DockerImageExists -ImageName $candidate) {
            return $candidate
        }
    }

    return "m.daocloud.io/docker.io/library/python:3.11-slim"
}

$effectiveReleaseRoot = if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    Join-Path $PSScriptRoot ("artifacts\agentthespire-{0}-release" -f $Target)
} else {
    $ReleaseRoot
}
$effectiveProjectName = if ([string]::IsNullOrWhiteSpace($ProjectName)) {
    "agentthespire-$Target-release"
} else {
    $ProjectName
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

$resolvedWorkstationPort = [int]$WorkstationPort
$resolvedWebPort = [int]$WebPort
$resolvedFrontendPort = [int]$FrontendPort

if ($Target -in @("hybrid", "frontend")) {
    $explicitWebBaseUrlProvided = $PSBoundParameters.ContainsKey("WebBaseUrl")
    $explicitWebReleaseRootProvided = $PSBoundParameters.ContainsKey("WebReleaseRoot")
    $shouldDeployHybridLocalWeb = $Target -eq "hybrid" -and ($DeployLocalWeb.IsPresent -or $explicitWebReleaseRootProvided)

    if ($Target -eq "hybrid" -and $explicitWebBaseUrlProvided -and $shouldDeployHybridLocalWeb) {
        throw "-WebBaseUrl 与 -DeployLocalWeb / -WebReleaseRoot 不能同时使用；请二选一。"
    }

    if ($explicitWebBaseUrlProvided) {
        if ([string]::IsNullOrWhiteSpace($WebBaseUrl)) {
            throw "显式传入 -WebBaseUrl 时不能为空。"
        }
        if (-not (Test-AbsoluteHttpUrl -Value $WebBaseUrl)) {
            throw "显式传入的 -WebBaseUrl 必须是完整的 http:// 或 https:// 地址，例如 http://127.0.0.1:$WebPort 或 https://your-web-api.example.com"
        }
    } elseif ($Target -eq "hybrid") {
        if ($shouldDeployHybridLocalWeb) {
            $WebBaseUrl = "http://127.0.0.1:{0}" -f $WebPort
        } else {
            throw "hybrid 默认不再自动联动本机 web-backend。请显式传入 -WebBaseUrl，或使用 -DeployLocalWeb（可配合 -WebReleaseRoot）启用本机 web 部署。"
        }
    } else {
        $WebBaseUrl = "http://127.0.0.1:{0}" -f $WebPort
    }
}

if ($Target -eq "hybrid" -and ($DeployLocalWeb.IsPresent -or $PSBoundParameters.ContainsKey("WebReleaseRoot"))) {
    Invoke-HybridLocalWebDeployment -HybridReleaseRoot $effectiveReleaseRoot -HybridProjectName $effectiveProjectName -ExplicitWebReleaseRoot $WebReleaseRoot
}

Assert-PathExists -Path $effectiveReleaseRoot -Label "release 目录"

$composeFile = Join-Path $effectiveReleaseRoot "docker-compose.yml"
$runtimeDir = Join-Path $effectiveReleaseRoot "runtime"
$envFile = Join-Path $runtimeDir ".env"
$targetNeedsPostgres = $Target -eq "web"
$targetUsesDockerInCurrentRelease = $Target -eq "web"
$resolvedPostgresImage = if ($targetNeedsPostgres) {
    Resolve-PostgresImage -PreferredImage $PostgresImage
} else {
    ""
}
$resolvedPythonBaseImage = if ($targetUsesDockerInCurrentRelease) {
    Resolve-PythonBaseImage -PreferredImage $PythonBaseImage
} else {
    ""
}
$shouldResetDatabase = $ResetDatabase.IsPresent
$null = New-Item -ItemType Directory -Path $runtimeDir -Force

if ($targetUsesDockerInCurrentRelease) {
    Assert-CommandExists -CommandName "docker"
    Assert-PathExists -Path $composeFile -Label "docker-compose.yml"
    Write-ComposeEnvFile -TargetName $Target -EnvPath $envFile -ResolvedPostgresImage $resolvedPostgresImage -ResolvedPythonBaseImage $resolvedPythonBaseImage
}

$workstationServiceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "workstation"
$frontendServiceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "frontend"
$webServiceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "web"

if ($Target -in @("hybrid", "workstation")) {
    $sourceConfigPath = Get-SourceConfigPath `
        -PreferredPath $ConfigPath `
        -RuntimeConfigPath (Join-Path $effectiveReleaseRoot "runtime\workstation.config.json") `
        -FallbackServiceDir $workstationServiceDir
} elseif ($Target -eq "web") {
    $sourceConfigPath = Get-SourceConfigPath `
        -PreferredPath $ConfigPath `
        -RuntimeConfigPath (Join-Path $effectiveReleaseRoot "runtime\web.config.json") `
        -FallbackServiceDir $webServiceDir
}

if ($Target -in @("hybrid", "workstation")) {
    $workstationConfig = New-RuntimeConfig -SourceConfigPath $sourceConfigPath -Mode "workstation" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
}

if ($Target -eq "hybrid" -or $Target -eq "workstation") {
    Write-RuntimeConfigFile -Config $workstationConfig -OutputPath (Join-Path $runtimeDir "workstation.config.json")
}

if ($Target -eq "web") {
    $webSourceConfigPath = $sourceConfigPath
    $webConfig = New-RuntimeConfig -SourceConfigPath $webSourceConfigPath -Mode "web" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
    Write-RuntimeConfigFile -Config $webConfig -OutputPath (Join-Path $runtimeDir "web.config.json")
}

if ($shouldResetDatabase) {
    if (-not $targetUsesDockerInCurrentRelease) {
        throw "-ResetDatabase 仅适用于 web 部署目标"
    }
    $resetReason = "检测到 -ResetDatabase，将删除 Docker 卷并重建数据库"
    Write-Host "$resetReason..."
    Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("down", "--volumes", "--remove-orphans")
}

if ($targetUsesDockerInCurrentRelease) {
    $buildServices = Get-BuildServices -TargetName $Target
    $servicesToBuild = @()

    foreach ($serviceName in $buildServices) {
        $imageName = Get-ComposeImageName -ComposeProjectName $effectiveProjectName -ServiceName $serviceName
        if ($RebuildImages) {
            Remove-DockerImageIfExists -ImageName $imageName
            $servicesToBuild += $serviceName
            continue
        }

        if ($ReuseImages) {
            if (-not (Test-DockerImageExists -ImageName $imageName)) {
                $servicesToBuild += $serviceName
            }
            continue
        }

        $servicesToBuild += $serviceName
    }

    if ($servicesToBuild.Count -gt 0) {
        # 默认以当前 release 为准重建镜像，避免发布目录更新后仍复用旧镜像。
        Write-Host "检测到需要构建本地镜像: $($servicesToBuild -join ', ')"
        Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("build") -Services $servicesToBuild
    }

    Write-Host "启动 Docker 部署..."
    Invoke-DockerCompose -BundleDir $effectiveReleaseRoot -ComposeFile $composeFile -EnvFile $envFile -ComposeProjectName $effectiveProjectName -ComposeArgs @("up", "-d", "--no-build") -Services $buildServices
}

$localProcesses = @()
Clear-LocalDeploymentState -ReleaseRoot $effectiveReleaseRoot

try {
    switch ($Target) {
        "workstation" {
            $process = Start-LocalWorkstationDeployment -ReleaseRoot $effectiveReleaseRoot -SourceConfigPath $sourceConfigPath -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
            if ($null -ne $process) {
                $localProcesses += [pscustomobject]@{
                    ServiceName = "workstation"
                    Process = $process
                    Port = $resolvedWorkstationPort
                }
            }
        }
        "frontend" {
            $frontendDist = Join-Path $frontendServiceDir "frontend\dist"
            $process = Start-LocalFrontendDeployment -ReleaseRoot $effectiveReleaseRoot -FrontendDist $frontendDist -ResolvedFrontendPort $resolvedFrontendPort -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
            if ($null -ne $process) {
                $localProcesses += [pscustomobject]@{
                    ServiceName = "frontend"
                    Process = $process
                    Port = $resolvedFrontendPort
                }
            }
        }
        "hybrid" {
            $workstationProcess = Start-LocalWorkstationDeployment -ReleaseRoot $effectiveReleaseRoot -SourceConfigPath $sourceConfigPath -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
            if ($null -ne $workstationProcess) {
                $localProcesses += [pscustomobject]@{
                    ServiceName = "workstation"
                    Process = $workstationProcess
                    Port = $resolvedWorkstationPort
                }
            }

            $frontendDist = Join-Path $frontendServiceDir "frontend\dist"
            $frontendProcess = Start-LocalFrontendDeployment -ReleaseRoot $effectiveReleaseRoot -FrontendDist $frontendDist -ResolvedFrontendPort $resolvedFrontendPort -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
            if ($null -ne $frontendProcess) {
                $localProcesses += [pscustomobject]@{
                    ServiceName = "frontend"
                    Process = $frontendProcess
                    Port = $resolvedFrontendPort
                }
            }
        }
    }
} catch {
    if ($localProcesses.Count -gt 0) {
        Stop-TrackedLocalProcesses -ReleaseRoot $effectiveReleaseRoot -ProcessEntries $localProcesses
    }
    Clear-LocalDeploymentState -ReleaseRoot $effectiveReleaseRoot
    throw
}

if ($localProcesses.Count -gt 0) {
    Write-LocalDeploymentState -ReleaseRoot $effectiveReleaseRoot -TargetName $Target -ProcessEntries $localProcesses
}

Write-Host ""
Write-Host "部署完成:"
Write-Host "  Target       : $Target"
Write-Host "  Release 目录 : $effectiveReleaseRoot"
if ($targetUsesDockerInCurrentRelease) {
    Write-Host "  Compose Env  : $envFile"
}
Write-Host "  复用已有镜像 : $($ReuseImages.IsPresent)"
Write-Host "  强制重建镜像 : $($RebuildImages.IsPresent)"
Write-Host "  重建数据库   : $shouldResetDatabase"
if ($targetNeedsPostgres) {
    Write-Host "  Postgres 镜像: $resolvedPostgresImage"
}
if ($targetUsesDockerInCurrentRelease) {
    Write-Host "  Python 基镜像: $resolvedPythonBaseImage"
}
switch ($Target) {
    "hybrid" {
        Write-Host "  工作站地址   : http://127.0.0.1:$WorkstationPort"
        Write-Host "  前端地址     : http://127.0.0.1:$FrontendPort"
        Write-Host "  Web API 基址 : $WebBaseUrl"
    }
    "workstation" {
        Write-Host "  工作站地址   : http://127.0.0.1:$WorkstationPort"
        if (-not [string]::IsNullOrWhiteSpace($WebBaseUrl)) {
            Write-Host "  Web API 基址 : $WebBaseUrl"
        }
    }
    "frontend" {
        Write-Host "  前端地址     : http://127.0.0.1:$FrontendPort"
        Write-Host "  Web API 基址 : $WebBaseUrl"
    }
    "web" {
        Write-Host "  Web 地址     : http://127.0.0.1:$WebPort"
    }
}
if ($localProcesses.Count -gt 0) {
    $processSummary = ($localProcesses | Where-Object { $null -ne $_ } | ForEach-Object {
        "{0} (PID {1})" -f $_.Process.ProcessName, $_.Process.Id
    }) -join ", "
    if (-not [string]::IsNullOrWhiteSpace($processSummary)) {
        Write-Host "  本机进程     : $processSummary"
    }
    Write-Host "  停止入口     : pwsh -File .\tools\latest\stop-deploy.ps1 $Target"
}
