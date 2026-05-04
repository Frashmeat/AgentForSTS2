from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "tools" / "tools.ps1"
KILL_LOCAL_PATH = REPO_ROOT / "tools" / "stop" / "kill-local.ps1"
RUN_PYTEST_PATH = REPO_ROOT / "tools" / "test" / "run-pytest.ps1"
GITIGNORE_PATH = REPO_ROOT / ".gitignore"
COMPOSE_APP_PATH = REPO_ROOT / "tools" / "latest" / "templates" / "compose.app.yml"
DEPLOY_APP_PATH = REPO_ROOT / "tools" / "latest" / "deploy-app.ps1"
LOGS_APP_PATH = REPO_ROOT / "tools" / "latest" / "logs-app.ps1"


def _run_tools(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["pwsh", "-NoProfile", "-File", str(SCRIPT_PATH), *args],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )


def _run_tools_inline(script: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["pwsh", "-NoProfile", "-Command", script],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )


def _run_windows_powershell_tools(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(SCRIPT_PATH), *args],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )


def test_tools_entry_help_lists_app_mainline_commands() -> None:
    completed = _run_tools("help")

    assert completed.returncode == 0, completed.stderr
    assert "环境部署" in completed.stdout
    assert "开发" in completed.stdout
    assert "Kill / 停止本机服务" in completed.stdout
    assert "打包" in completed.stdout
    assert "部署" in completed.stdout
    assert "stop local" in completed.stdout
    assert "stop app" in completed.stdout
    assert "logs app" in completed.stdout
    assert "package app" in completed.stdout
    assert "deploy app" in completed.stdout
    assert "latest package" not in completed.stdout
    assert "latest deploy" not in completed.stdout
    assert "stop deploy" not in completed.stdout
    assert "docker web" not in completed.stdout
    assert "docker workstation" not in completed.stdout


def test_tools_entry_help_works_in_windows_powershell_when_available() -> None:
    if shutil.which("powershell") is None:
        return

    completed = _run_windows_powershell_tools("-Help")

    assert completed.returncode == 0, completed.stderr
    assert "deploy app" in completed.stdout
    assert "latest deploy" not in completed.stdout


def test_tools_entry_stop_local_routes_to_stop_directory_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'stop' -ResolvedAction 'local'"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\stop\\kill-local.ps1"


def test_tools_entry_stop_defaults_to_local_kill_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'stop' -ResolvedAction ''"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\stop\\kill-local.ps1"


def test_tools_entry_stop_app_routes_to_app_stop_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative + '|' + ($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'stop' -ResolvedAction 'app' -ResolvedArgs @('-Help')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\latest\\stop-app.ps1|-Help"


def test_kill_local_script_includes_current_app_stop_flow() -> None:
    source = KILL_LOCAL_PATH.read_text(encoding="utf-8")

    assert 'Join-Path $repoRoot "runtime\\app-deploy-state.json"' in source
    assert 'Join-Path $toolsRoot "latest\\stop-app.ps1"' in source
    assert "Invoke-CurrentAppStopScript" in source
    assert "Register-PortsFromAppDeployState" in source


def test_tools_entry_package_app_routes_to_app_package_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative + '|' + ($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'package' -ResolvedAction 'app' -ResolvedArgs @('-NoZip')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\latest\\package-app.ps1|-NoZip"


def test_tools_entry_deploy_app_routes_to_app_deploy_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative + '|' + ($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'deploy' -ResolvedAction 'app' -ResolvedArgs @('-DryRun')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\latest\\deploy-app.ps1|-DryRun"


def test_tools_entry_logs_app_routes_to_app_logs_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative + '|' + ($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'logs' -ResolvedAction 'app' -ResolvedArgs @('-Service', 'web')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\latest\\logs-app.ps1|-Service,web"


def test_tools_entry_catalog_script_paths_exist() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "$catalog = Get-CommandCatalog; "
        "$missing = foreach ($group in $catalog) { "
        "foreach ($command in $group.Commands) { "
        "if (-not (Test-Path -LiteralPath $command.ScriptPath)) { $command.InvocationName + '|' + $command.ScriptPath } "
        "} }; "
        "[Console]::Out.Write(($missing -join ';'))"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == ""


def test_tools_entry_test_routes_to_project_pytest_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "$relative = [System.IO.Path]::GetRelativePath((Get-Location).Path, $Path); "
        "[Console]::Out.Write($relative + '|' + ($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'test' -ResolvedAction '' "
        "-ResolvedArgs @('backend/tests/test_tools_entry_script.py', '-q')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "tools\\test\\run-pytest.ps1|backend/tests/test_tools_entry_script.py,-q"


def test_kill_local_uses_repo_and_tools_roots_after_stop_directory_move() -> None:
    source = KILL_LOCAL_PATH.read_text(encoding="utf-8-sig")

    assert '$toolsRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))' in source
    assert '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $toolsRoot ".."))' in source
    assert 'Join-Path $repoRoot "runtime\\workstation.config.json"' in source
    assert 'Join-Path $repoRoot "runtime\\split-local-state.json"' in source
    assert 'Join-Path $toolsRoot "latest\\artifacts"' in source
    assert 'Join-Path $PSScriptRoot "latest\\artifacts"' not in source
    assert 'Join-Path $PSScriptRoot "..\\runtime' not in source
    assert "function Stop-ArtifactResidentProcesses" in source
    assert "function Test-IsArtifactResidentProcess" in source
    assert "function Get-ProcessCurrentDirectory" in source
    assert "AtsProcessDirectoryReader" in source
    assert "CurrentDirectory = Get-ProcessCurrentDirectory" in source
    assert "命令行、可执行路径或当前工作目录指向 tools\\latest\\artifacts" in source
    assert "Stop-ArtifactResidentProcesses" in source


def test_run_pytest_uses_backend_virtualenv_python() -> None:
    source = RUN_PYTEST_PATH.read_text(encoding="utf-8-sig")

    assert 'Join-Path $repoRoot "backend\\.venv\\Scripts\\python.exe"' in source
    assert "& $backendPython -m pytest @PytestArgs" in source
    assert "tools.ps1 install" in source
    assert "repository-root .venv" not in source


def test_root_virtualenv_and_old_env_files_are_ignored() -> None:
    ignored_entries = {
        line.strip()
        for line in GITIGNORE_PATH.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }

    assert ".venv/" in ignored_entries
    assert "backend/.venv/" in ignored_entries
    assert ".env.web" in ignored_entries
    assert ".env.workstation" in ignored_entries


def test_app_compose_contains_single_docker_topology() -> None:
    source = COMPOSE_APP_PATH.read_text(encoding="utf-8")

    assert "postgres:" in source
    assert "web:" in source
    assert "web-workstation:" in source
    assert "ATS_DOCKER_NETWORK" in source
    assert "http://web-workstation" not in source
    web_workstation_block = source.split("web-workstation:", 1)[1].split("  web:", 1)[0]
    assert "expose:" in web_workstation_block
    assert "ports:" not in web_workstation_block


def test_deploy_app_dry_run_does_not_require_frontend_dist() -> None:
    source = DEPLOY_APP_PATH.read_text(encoding="utf-8-sig")

    assert 'if (-not $DryRun) {' in source
    assert 'Assert-PathExists -Path $layout.LocalFrontendDist -Label "frontend/dist"' in source
    assert 'Copy-Item -LiteralPath $paths.RuntimeConfig -Destination' in source


def test_deploy_app_verifies_docker_web_stack_after_compose_up() -> None:
    source = DEPLOY_APP_PATH.read_text(encoding="utf-8-sig")

    assert "function Assert-DockerComposeServicesRunning" in source
    assert "function Invoke-WebRuntimeBootstrap" in source
    assert '$expectedServices = @("postgres", "web-workstation", "web")' in source
    assert "[string[]]$ComposeArgs" in source
    assert 'Invoke-DockerCompose -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv -ComposeArgs @("up", "-d", "--no-build")' in source
    assert "[string[]]$Args" not in source
    assert "Assert-DockerComposeServicesRunning -AppConfig $appConfig -Layout $layout -EnvFile $paths.DockerEnv" in source
    assert 'Invoke-DockerComposeExec -AppConfig $AppConfig -Layout $Layout -EnvFile $EnvFile -ExecArgs @("web", "alembic", "upgrade", "head")' in source
    assert 'Invoke-DockerComposeExec -AppConfig $AppConfig -Layout $Layout -EnvFile $EnvFile -ExecArgs @("web", "python", "tools/bootstrap_web_runtime.py", "--ensure-default-admin")' in source
    assert "Docker web 栈: postgres / web-workstation / web 已启动" in source
    assert "默认管理员   : admin / admin@example.com / admin123456（每次部署收敛）" in source
    assert "日志入口     : powershell -File .\\tools\\tools.ps1 logs app" in source
    assert "日志目录     : $logRoot" in source


def test_logs_app_exposes_local_and_docker_log_sources() -> None:
    source = LOGS_APP_PATH.read_text(encoding="utf-8-sig")

    assert 'Join-Path $Layout.ConfigRoot "logs"' in source
    assert '"frontend"' in source
    assert '"local-workstation"' in source
    assert '"postgres", "web-workstation", "web"' in source
    assert '"logs"' in source
    assert '"--tail", [string]$LineCount' in source
    assert '$dockerArgs += "-f"' in source
