from __future__ import annotations

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


def test_tools_entry_help_lists_stop_commands() -> None:
    completed = _run_tools("help")

    assert completed.returncode == 0, completed.stderr
    assert "stop local" in completed.stdout
    assert "stop deploy" in completed.stdout


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


def test_tools_entry_latest_package_help_routes_to_package_script() -> None:
    completed = _run_tools("latest", "package", "hybrid", "-Help")

    assert completed.returncode == 0, completed.stderr
    assert "package-release.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout


def test_tools_entry_latest_package_hybrid_profile_keeps_target_args() -> None:
    completed = _run_tools_inline(
        ". .\\tools\\tools.ps1 help; "
        "$catalog = Get-CommandCatalog; "
        "$command = Find-MenuCommand -Catalog $catalog -GroupKey 'latest' -ActionKey 'package'; "
        "$profile = $command.Profiles | Where-Object { $_.Key -eq 'hybrid' } | Select-Object -First 1; "
        "$resolved = @(Resolve-ProfileArgs -Profile $profile); "
        "[Console]::Out.Write(($resolved -join ','))"
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.endswith("hybrid")


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
