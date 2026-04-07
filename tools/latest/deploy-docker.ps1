<#
.SYNOPSIS
按目标启动 AgentTheSpire 的混合部署。

.DESCRIPTION
读取 release bundle、生成运行时配置，并按目标在本机或 Docker 中启动对应服务。
直接执行脚本且不传任何参数时，会默认显示本帮助而不是立即启动部署。

.PARAMETER Target
部署目标。可选 full / hybrid / workstation / frontend / web。

.PARAMETER ReleaseRoot
release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。

.PARAMETER ConfigPath
运行时配置文件路径。默认读取仓库根目录 config.json。

.PARAMETER ProjectName
Compose 项目名。默认按 agentthespire-<target>-release 生成。

.PARAMETER WorkstationPort
工作站端口。默认 7860。

.PARAMETER WebPort
Web 端口。默认 7870。

.PARAMETER FrontendPort
前端静态站端口。默认 8080。

.PARAMETER WebBaseUrl
前端运行时写入的 Web API 基地址。`hybrid` / `frontend` 未显式传入时默认使用本机 `http://127.0.0.1:<WebPort>`，并可联动部署本机 Docker `web-backend`；显式传入后改为指向指定地址。

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

.PARAMETER ResetDatabase
重建数据库。full 默认会执行；web 目标可显式开启。

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
pwsh -File .\tools\latest\deploy-docker.ps1 hybrid

.EXAMPLE
pwsh -File .\tools\latest\deploy-docker.ps1 web -ResetDb -dbn agentthespire
#>
[CmdletBinding()]
param(
    # 基础参数
    [Parameter(Position = 0, HelpMessage = "部署目标。可选 full / hybrid / workstation / frontend / web。")]
    [Alias("t")]
    [ValidateSet("full", "hybrid", "workstation", "frontend", "web")]
    [string]$Target = "workstation",

    [Parameter(HelpMessage = "release 目录。默认使用 tools/latest/artifacts/agentthespire-<target>-release。")]
    [Alias("r")]
    [string]$ReleaseRoot = "",

    [Parameter(HelpMessage = "运行时配置文件路径。默认读取仓库根目录 config.json。")]
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

    [Parameter(HelpMessage = "前端运行时写入的 Web API 基地址。`hybrid` 未显式传入时默认使用本机 http://127.0.0.1:<WebPort>，并自动联动部署本机 web-backend。")]
    [Alias("wb")]
    [string]$WebBaseUrl = "",

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

    # 行为开关
    [Parameter(HelpMessage = "重建数据库。full 默认会执行；web 目标可显式开启。")]
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

if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    $ConfigPath = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path "config.json"
}

if ($ReuseImages.IsPresent -and $RebuildImages.IsPresent) {
    throw "-ReuseImages 与 -RebuildImages 不能同时使用。"
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

function Ensure-LocalBackendRuntimePython {
    param(
        [string]$BackendRoot,
        [string]$RepoRoot
    )

    $requiredModules = @("uvicorn", "fastapi", "sqlalchemy")
    $releaseVenvPython = Join-Path $BackendRoot ".venv\Scripts\python.exe"
    if (Test-PythonModulesAvailable -PythonExe $releaseVenvPython -Modules $requiredModules) {
        return $releaseVenvPython
    }

    $repoVenvPython = Join-Path $RepoRoot "backend\.venv\Scripts\python.exe"
    if (Test-PythonModulesAvailable -PythonExe $repoVenvPython -Modules $requiredModules) {
        return $repoVenvPython
    }

    $bootstrapPython = Resolve-PythonCommand -RepoRoot $RepoRoot
    $requirementsPath = Join-Path $BackendRoot "requirements.txt"
    if (-not (Test-Path -LiteralPath $requirementsPath)) {
        throw "缺少 backend/requirements.txt，无法准备本地 Python 运行时：$requirementsPath"
    }

    Write-Host "检测到本地 workstation Python 依赖未就绪，开始准备 release 运行时..."
    Write-Host "  BackendRoot    : $BackendRoot"
    Write-Host "  Bootstrap Py   : $bootstrapPython"

    if (-not (Test-Path -LiteralPath $releaseVenvPython)) {
        & $bootstrapPython -m venv (Join-Path $BackendRoot ".venv") | Out-Host
        if ($LASTEXITCODE -ne 0) {
            throw "创建 release backend/.venv 失败。"
        }
    }

    Write-Host "  升级 pip..."
    Invoke-PipInstallWithFallback -PythonExe $releaseVenvPython -WorkingDirectory $BackendRoot -PipArguments @("--upgrade", "pip")
    Write-Host "  安装 requirements.txt..."
    Invoke-PipInstallWithFallback -PythonExe $releaseVenvPython -WorkingDirectory $BackendRoot -PipArguments @("-r", "requirements.txt")

    if (-not (Test-PythonModulesAvailable -PythonExe $releaseVenvPython -Modules $requiredModules)) {
        throw "release backend/.venv 依赖安装后仍不完整，请检查 pip 输出。"
    }

    return $releaseVenvPython
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
    [System.IO.File]::WriteAllText($StdOutPath, "", $utf8NoBom)
    [System.IO.File]::WriteAllText($StdErrPath, "", $utf8NoBom)

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = New-ProcessStartInfo -FilePath $FilePath -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory
    $process.EnableRaisingEvents = $true

    $stdoutWriter = New-Object System.IO.StreamWriter($StdOutPath, $false, $utf8NoBom)
    $stdoutWriter.AutoFlush = $true
    $stderrWriter = New-Object System.IO.StreamWriter($StdErrPath, $false, $utf8NoBom)
    $stderrWriter.AutoFlush = $true

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
    return $process
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
    param([string]$HybridReleaseRoot)

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

function Invoke-HybridLocalWebDeployment {
    param(
        [string]$HybridReleaseRoot,
        [string]$HybridProjectName
    )

    $webReleaseRoot = Get-DefaultHybridWebReleaseRoot -HybridReleaseRoot $HybridReleaseRoot
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
        [string]$FallbackServiceDir
    )

    if (Test-Path -LiteralPath $PreferredPath) {
        return (Resolve-Path $PreferredPath).Path
    }

    $fallback = Join-Path $FallbackServiceDir "config.example.json"
    if (Test-Path -LiteralPath $fallback) {
        return $fallback
    }

    throw "未找到可用配置文件。请提供 config.json，或先执行打包脚本生成 release bundle。"
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
        # workstation/full 中的工作站进程不应暴露 web 平台路由。
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
        [string]$ServiceConfigPath,
        [string]$RuntimeMirrorPath = ""
    )

    Write-RuntimeConfigFile -Config $Config -OutputPath $ServiceConfigPath

    if (-not [string]::IsNullOrWhiteSpace($RuntimeMirrorPath)) {
        Write-RuntimeConfigFile -Config $Config -OutputPath $RuntimeMirrorPath
    }
}

function Write-ComposeEnvFile {
    param(
        [string]$TargetName,
        [string]$EnvPath,
        [string]$ResolvedPostgresImage
    )

    # Compose 模板的端口、数据库和镜像选择都从 runtime/.env 注入，bundle 本身保持静态。
    $lines = switch ($TargetName) {
        "full" {
            @(
                "ATS_WORKSTATION_PORT=$WorkstationPort"
                "ATS_WEB_PORT=$WebPort"
                "ATS_POSTGRES_HOST_PORT=$PostgresHostPort"
                "ATS_POSTGRES_DB=$PostgresDb"
                "ATS_POSTGRES_USER=$PostgresUser"
                "ATS_POSTGRES_PASSWORD=$PostgresPassword"
                "ATS_POSTGRES_IMAGE=$ResolvedPostgresImage"
            )
        }
        "workstation" {
            @("ATS_WORKSTATION_PORT=$WorkstationPort")
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
        & docker compose --project-name $ComposeProjectName --env-file $EnvFile -f $ComposeFile @ComposeArgs @Services
        if ($LASTEXITCODE -ne 0) {
            throw "docker compose 执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
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
    $serviceConfigPath = Join-Path $serviceRoot "config.json"
    $runtimeMirrorPath = Join-Path (Join-Path $ReleaseRoot "runtime") "workstation.config.json"

    Assert-PathExists -Path $backendRoot -Label "workstation backend 目录"
    Assert-PathExists -Path $frontendDist -Label "workstation frontend/dist 目录"

    $workstationConfig = New-RuntimeConfig -SourceConfigPath $SourceConfigPath -Mode "workstation" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
    Write-LocalServiceConfig -Config $workstationConfig -ServiceConfigPath $serviceConfigPath -RuntimeMirrorPath $runtimeMirrorPath
    Write-FrontendRuntimeConfig -OutputPath (Join-Path $frontendDist "runtime-config.js") `
        -ResolvedWorkstationBaseUrl ("http://127.0.0.1:{0}" -f $ResolvedWorkstationPort) `
        -ResolvedWorkstationWsBaseUrl ("ws://127.0.0.1:{0}" -f $ResolvedWorkstationPort) `
        -ResolvedWebBaseUrl $ResolvedWebBaseUrl

    $logPaths = Get-ProcessLogPaths -ReleaseRoot $ReleaseRoot -ServiceName "workstation"
    Stop-ProcessListeningOnPort -Port $ResolvedWorkstationPort
    $pythonCommand = if ([Environment]::GetEnvironmentVariable("ATS_SKIP_LOCAL_READY_CHECK") -eq "1") {
        Resolve-PythonCommand -BackendRoot $backendRoot -RepoRoot $RepoRoot
    } else {
        Ensure-LocalBackendRuntimePython -BackendRoot $backendRoot -RepoRoot $RepoRoot
    }
    Write-Host "启动本机 workstation-backend..."
    Write-Host "  Python 解释器 : $pythonCommand"
    Write-Host "  stdout 日志   : $($logPaths.StdOut)"
    Write-Host "  stderr 日志   : $($logPaths.StdErr)"
    $process = Start-LocalProcessWithMirroredLogs -ServiceName "workstation" -FilePath $pythonCommand -ArgumentList @(
        "-m",
        "uvicorn",
        "main_workstation:app",
        "--host",
        "127.0.0.1",
        "--port",
        "$ResolvedWorkstationPort"
    ) -WorkingDirectory $backendRoot -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr
    Start-LogViewerWindow -ReleaseRoot $ReleaseRoot -ServiceName "workstation" -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr

    try {
        Wait-LocalServiceReady -Process $process -ServiceName "workstation-backend" -Url ("http://127.0.0.1:{0}/api/config" -f $ResolvedWorkstationPort) -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr
    } catch {
        try {
            Stop-Process -Id $process.Id -Force -ErrorAction Stop
        } catch {
        }
        throw
    }

    return $process
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
    $process = Start-LocalProcessWithMirroredLogs -ServiceName "frontend" -FilePath $pythonCommand -ArgumentList @(
        "-m",
        "http.server",
        "$ResolvedFrontendPort",
        "--bind",
        "127.0.0.1",
        "--directory",
        $FrontendDist
    ) -WorkingDirectory $FrontendDist -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr
    Start-LogViewerWindow -ReleaseRoot $ReleaseRoot -ServiceName "frontend" -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr

    try {
        Wait-LocalServiceReady -Process $process -ServiceName "frontend-static" -Url ("http://127.0.0.1:{0}/runtime-config.js" -f $ResolvedFrontendPort) -StdOutPath $logPaths.StdOut -StdErrPath $logPaths.StdErr
    } catch {
        try {
            Stop-Process -Id $process.Id -Force -ErrorAction Stop
        } catch {
        }
        throw
    }

    return $process
}

function Get-BuildServices {
    param(
        [ValidateSet("full", "hybrid", "workstation", "frontend", "web")]
        [string]$TargetName
    )

    switch ($TargetName) {
        "full" { return @("web") }
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

    if ($explicitWebBaseUrlProvided) {
        if ([string]::IsNullOrWhiteSpace($WebBaseUrl)) {
            throw "显式传入 -WebBaseUrl 时不能为空；不传该参数则会默认使用本机 http://127.0.0.1:$WebPort"
        }
        if (-not (Test-AbsoluteHttpUrl -Value $WebBaseUrl)) {
            throw "显式传入的 -WebBaseUrl 必须是完整的 http:// 或 https:// 地址，例如 http://127.0.0.1:$WebPort 或 https://your-web-api.example.com"
        }
    } else {
        $WebBaseUrl = "http://127.0.0.1:{0}" -f $WebPort
    }
}

if ($Target -eq "hybrid" -and (-not $PSBoundParameters.ContainsKey("WebBaseUrl"))) {
    Invoke-HybridLocalWebDeployment -HybridReleaseRoot $ReleaseRoot -HybridProjectName $ProjectName
}

Assert-PathExists -Path $effectiveReleaseRoot -Label "release 目录"

$composeFile = Join-Path $effectiveReleaseRoot "docker-compose.yml"
$runtimeDir = Join-Path $effectiveReleaseRoot "runtime"
$envFile = Join-Path $runtimeDir ".env"
$targetNeedsPostgres = $Target -in @("full", "web")
$targetUsesDockerInCurrentRelease = $Target -in @("full", "web")
$resolvedPostgresImage = if ($targetNeedsPostgres) {
    Resolve-PostgresImage -PreferredImage $PostgresImage
} else {
    ""
}
$shouldResetDatabase = $Target -eq "full" -or $ResetDatabase.IsPresent
$null = New-Item -ItemType Directory -Path $runtimeDir -Force

if ($targetUsesDockerInCurrentRelease) {
    Assert-CommandExists -CommandName "docker"
    Assert-PathExists -Path $composeFile -Label "docker-compose.yml"
    Write-ComposeEnvFile -TargetName $Target -EnvPath $envFile -ResolvedPostgresImage $resolvedPostgresImage
}

$workstationServiceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "workstation"
$frontendServiceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "frontend"
$webServiceDir = Join-Path (Join-Path $effectiveReleaseRoot "services") "web"

if ($Target -in @("full", "hybrid", "workstation")) {
    $sourceConfigPath = Get-SourceConfigPath -PreferredPath $ConfigPath -FallbackServiceDir $workstationServiceDir
} elseif ($Target -eq "web") {
    $sourceConfigPath = Get-SourceConfigPath -PreferredPath $ConfigPath -FallbackServiceDir $webServiceDir
}

if ($Target -in @("full", "hybrid", "workstation")) {
    $workstationConfig = New-RuntimeConfig -SourceConfigPath $sourceConfigPath -Mode "workstation" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
}

if ($Target -eq "full") {
    Write-RuntimeConfigFile -Config $workstationConfig -OutputPath (Join-Path $runtimeDir "workstation.config.json")
} elseif ($Target -eq "hybrid" -or $Target -eq "workstation") {
    Write-RuntimeConfigFile -Config $workstationConfig -OutputPath (Join-Path $runtimeDir "workstation.config.json")
}

if ($Target -eq "full" -or $Target -eq "web") {
    if ($Target -eq "full") {
        $webSourceConfigPath = Get-SourceConfigPath -PreferredPath $ConfigPath -FallbackServiceDir $webServiceDir
    } else {
        $webSourceConfigPath = $sourceConfigPath
    }
    $webConfig = New-RuntimeConfig -SourceConfigPath $webSourceConfigPath -Mode "web" -DbUser $PostgresUser -DbPassword $PostgresPassword -DbName $PostgresDb
    Write-RuntimeConfigFile -Config $webConfig -OutputPath (Join-Path $runtimeDir "web.config.json")
}

if ($shouldResetDatabase) {
    if (-not $targetUsesDockerInCurrentRelease) {
        throw "-ResetDatabase 仅适用于包含 Web 后端数据库的部署目标: full / web"
    }
    # full 目标默认重建数据库，避免同机联调时复用旧卷里的脏迁移状态。
    $resetReason = if ($Target -eq "full" -and (-not $ResetDatabase.IsPresent)) {
        "检测到 full 目标，默认重建数据库"
    } else {
        "检测到 -ResetDatabase，将删除 Docker 卷并重建数据库"
    }
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

switch ($Target) {
    "workstation" {
        $localProcesses += Start-LocalWorkstationDeployment -ReleaseRoot $effectiveReleaseRoot -SourceConfigPath $sourceConfigPath -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
    }
    "frontend" {
        $frontendDist = Join-Path $frontendServiceDir "frontend\dist"
        $localProcesses += Start-LocalFrontendDeployment -ReleaseRoot $effectiveReleaseRoot -FrontendDist $frontendDist -ResolvedFrontendPort $resolvedFrontendPort -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
    }
    "hybrid" {
        $localProcesses += Start-LocalWorkstationDeployment -ReleaseRoot $effectiveReleaseRoot -SourceConfigPath $sourceConfigPath -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
        $frontendDist = Join-Path $frontendServiceDir "frontend\dist"
        $localProcesses += Start-LocalFrontendDeployment -ReleaseRoot $effectiveReleaseRoot -FrontendDist $frontendDist -ResolvedFrontendPort $resolvedFrontendPort -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl $WebBaseUrl -RepoRoot $repoRoot
    }
    "full" {
        $localProcesses += Start-LocalWorkstationDeployment -ReleaseRoot $effectiveReleaseRoot -SourceConfigPath $sourceConfigPath -ResolvedWorkstationPort $resolvedWorkstationPort -ResolvedWebBaseUrl ("http://127.0.0.1:{0}" -f $resolvedWebPort) -RepoRoot $repoRoot
    }
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
switch ($Target) {
    "full" {
        Write-Host "  工作站地址   : http://127.0.0.1:$WorkstationPort"
        Write-Host "  Web 地址     : http://127.0.0.1:$WebPort"
    }
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
        "{0} (PID {1})" -f $_.ProcessName, $_.Id
    }) -join ", "
    if (-not [string]::IsNullOrWhiteSpace($processSummary)) {
        Write-Host "  本机进程     : $processSummary"
    }
}
