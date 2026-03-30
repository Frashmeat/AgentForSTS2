"""Tests for approval runtime configuration wiring."""
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from approval.runtime import get_approval_executor, reset_approval_runtime


@pytest.fixture(autouse=True)
def approval_runtime_isolation():
    reset_approval_runtime()
    try:
        yield
    finally:
        reset_approval_runtime()


def test_runtime_uses_configured_allowed_roots_and_commands(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    from approval import runtime

    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    configured_root = repo_root / "mods"
    configured_root.mkdir()

    monkeypatch.setattr(runtime, "_repo_root", lambda: repo_root)
    monkeypatch.setattr(
        runtime,
        "get_config",
        lambda: {
            "approval": {
                "allowed_roots": ["mods"],
                "allowed_commands": ["dotnet", ["python", "-m"]],
            }
        },
    )
    runtime.reset_approval_runtime()

    executor = get_approval_executor()

    assert executor.allowed_roots == [configured_root.resolve()]
    assert executor.allowed_commands == [["dotnet"], ["python", "-m"]]


def test_runtime_falls_back_to_repo_root_when_allowed_roots_not_configured(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    from approval import runtime

    repo_root = tmp_path / "repo"
    repo_root.mkdir()

    monkeypatch.setattr(runtime, "_repo_root", lambda: repo_root)
    monkeypatch.setattr(
        runtime,
        "get_config",
        lambda: {"approval": {"allowed_roots": [], "allowed_commands": []}},
    )
    runtime.reset_approval_runtime()

    executor = get_approval_executor()

    assert executor.allowed_roots == [repo_root.resolve()]
    assert executor.allowed_commands == []
