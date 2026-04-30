from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "tools" / "tools.ps1"
KILL_LOCAL_PATH = REPO_ROOT / "tools" / "stop" / "kill-local.ps1"
PACKAGE_RELEASE_PATH = REPO_ROOT / "tools" / "latest" / "package-release.ps1"
RUN_PYTEST_PATH = REPO_ROOT / "tools" / "test" / "run-pytest.ps1"
GITIGNORE_PATH = REPO_ROOT / ".gitignore"


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


def test_tools_entry_help_lists_stop_commands() -> None:
    completed = _run_tools("help")

    assert completed.returncode == 0, completed.stderr
    assert "环境部署" in completed.stdout
    assert "开发" in completed.stdout
    assert "Kill / 停止本机服务" in completed.stdout
    assert "打包" in completed.stdout
    assert "部署" in completed.stdout
    assert "stop local" in completed.stdout
    assert "stop deploy" in completed.stdout


def test_tools_entry_help_works_in_windows_powershell_when_available() -> None:
    if shutil.which("powershell") is None:
        return

    completed = _run_windows_powershell_tools("-Help")

    assert completed.returncode == 0, completed.stderr
    assert "stop deploy" in completed.stdout


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


def test_package_release_cleanup_failure_points_to_stop_local() -> None:
    source = PACKAGE_RELEASE_PATH.read_text(encoding="utf-8-sig")

    assert "清理 release 目录失败" in source
    assert "powershell -File .\\tools\\tools.ps1 stop local" in source


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


def test_run_pytest_uses_backend_virtualenv_python() -> None:
    source = RUN_PYTEST_PATH.read_text(encoding="utf-8-sig")

    assert 'Join-Path $repoRoot "backend\\.venv\\Scripts\\python.exe"' in source
    assert "& $backendPython -m pytest @PytestArgs" in source
    assert "tools.ps1 install" in source
    assert "repository-root .venv" not in source


def test_root_virtualenv_is_ignored_to_keep_backend_venv_as_single_source() -> None:
    ignored_entries = {
        line.strip()
        for line in GITIGNORE_PATH.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }

    assert ".venv/" in ignored_entries
    assert "backend/.venv/" in ignored_entries


def test_tools_entry_stop_deploy_help_routes_to_stop_deploy_script() -> None:
    completed = _run_tools("stop", "deploy", "-Help")

    assert completed.returncode == 0, completed.stderr
    assert "stop-deploy.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout


def test_tools_entry_stop_deploy_named_args_do_not_trigger_default_target_injection() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "[Console]::Out.Write(($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'stop' -ResolvedAction 'deploy' "
        "-ResolvedArgs @('-ReleaseRoot', 'foo')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "-ReleaseRoot,foo"


def test_tools_entry_stop_deploy_uses_default_target_when_no_args_are_provided() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "[Console]::Out.Write(($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'stop' -ResolvedAction 'deploy'"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "hybrid"


def test_tools_entry_stop_deploy_menu_profiles_do_not_duplicate_default_target() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "$catalog = Get-CommandCatalog; "
        "$command = Find-MenuCommand -Catalog $catalog -GroupKey 'stop' -ActionKey 'deploy'; "
        "$items = foreach ($profile in $command.Profiles) { "
        "$resolved = @(Resolve-MenuInvocationArgs -Command $command -Profile $profile); "
        "$profile.Key + ':' + ($resolved -join ',') "
        "}; "
        "[Console]::Out.Write(($items -join ';'))"
    )

    assert completed.returncode == 0, completed.stderr
    assert "hybrid:hybrid" in completed.stdout
    assert "workstation:workstation" in completed.stdout
    assert "frontend:frontend" in completed.stdout
    assert "web:web" in completed.stdout
    assert "help:-Help" in completed.stdout
    assert "hybrid,workstation" not in completed.stdout


def test_tools_entry_latest_package_help_routes_to_package_script() -> None:
    completed = _run_tools("latest", "package", "hybrid", "-Help")

    assert completed.returncode == 0, completed.stderr
    assert "package-release.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout


def test_tools_entry_latest_package_hybrid_profile_keeps_target_args() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "$catalog = Get-CommandCatalog; "
        "$command = Find-MenuCommand -Catalog $catalog -GroupKey 'latest' -ActionKey 'package'; "
        "$profile = $command.Profiles | Where-Object { $_.Key -eq 'hybrid' } | Select-Object -First 1; "
        "$resolved = @(Resolve-MenuInvocationArgs -Command $command -Profile $profile); "
        "[Console]::Out.Write(($resolved -join ','))"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.endswith("hybrid")


def test_tools_entry_latest_deploy_web_reset_db_profile_keeps_destructive_args_explicit() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "$catalog = Get-CommandCatalog; "
        "$command = Find-MenuCommand -Catalog $catalog -GroupKey 'latest' -ActionKey 'deploy'; "
        "$profile = $command.Profiles | Where-Object { $_.Key -eq 'web-reset-db' } | Select-Object -First 1; "
        "$resolved = @(Resolve-MenuInvocationArgs -Command $command -Profile $profile); "
        "[Console]::Out.Write(($resolved -join ','))"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "web,-ResetDb"


def test_tools_entry_dev_reset_web_db_routes_to_reset_script() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "[Console]::Out.Write((Split-Path -Leaf $Path) + '|' + ($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'dev' -ResolvedAction 'reset-web-db'"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "reset_web_database_with_test_data.ps1|-Yes"


def test_tools_entry_latest_deploy_web_debug_profile_keeps_debug_args() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "$catalog = Get-CommandCatalog; "
        "$command = Find-MenuCommand -Catalog $catalog -GroupKey 'latest' -ActionKey 'deploy'; "
        "$profile = $command.Profiles | Where-Object { $_.Key -eq 'web-debug' } | Select-Object -First 1; "
        "$resolved = @(Resolve-ProfileArgs -Profile $profile); "
        "[Console]::Out.Write(($resolved -join ','))"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "web,-Debug"


def test_tools_entry_latest_deploy_translates_debug_to_debug_test_data() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "[Console]::Out.Write(($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'latest' -ResolvedAction 'deploy' "
        "-ResolvedArgs @('hybrid', '-DeployLocalWeb', '-Debug')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "hybrid,-DeployLocalWeb,-DebugTestData"


def test_tools_entry_latest_deploy_common_debug_enables_debug_test_data() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help *> $null; "
        "$DebugPreference = 'Continue'; "
        "function Invoke-TargetScript { param([string]$Path, [string[]]$Arguments = @()) "
        "[Console]::Out.Write(($Arguments -join ',')) }; "
        "$catalog = Get-CommandCatalog; "
        "Invoke-Route -Catalog $catalog -ResolvedGroup 'latest' -ResolvedAction 'deploy' "
        "-ResolvedArgs @('hybrid', '-DeployLocalWeb')"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout == "hybrid,-DeployLocalWeb,-DebugTestData"
