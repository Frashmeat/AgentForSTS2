from __future__ import annotations

import shutil
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "tools" / "tools.ps1"


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
