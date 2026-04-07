from __future__ import annotations

import asyncio
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from llm.agent_backends import claude_cli


def test_claude_cli_backend_resolves_launcher_before_run_streaming(monkeypatch, tmp_path: Path):
    captured: dict[str, object] = {}

    async def fake_run_streaming(cmd, **kwargs):
        captured["cmd"] = cmd
        captured["cwd"] = kwargs["cwd"]
        return (["ok"], [])

    monkeypatch.setattr(claude_cli, "_resolve_claude_launcher", lambda: ["C:/tools/claude.cmd"])
    monkeypatch.setattr(claude_cli, "run_streaming", fake_run_streaming)

    result = asyncio.run(claude_cli.run("hello", tmp_path, {}))

    assert result == "ok"
    assert captured["cmd"][0] == "C:/tools/claude.cmd"
    assert captured["cwd"] == tmp_path


@pytest.mark.skipif(os.name != "nt", reason="仅在 Windows 上复现 PowerShell 与 CreateProcess 差异")
def test_powershell_can_resolve_claude_ps1_while_python_subprocess_cannot_spawn_plain_claude(tmp_path: Path):
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    ps1_path = bin_dir / "claude.ps1"
    ps1_path.write_text("Write-Output 'fake claude'\n", encoding="utf-8")

    env = os.environ.copy()
    env["PATH"] = str(bin_dir) + os.pathsep + env.get("PATH", "")

    powershell_lookup = subprocess.run(
        [
            "pwsh",
            "-NoProfile",
            "-Command",
            "$cmd = Get-Command claude; Write-Output ($cmd.CommandType.ToString() + '|' + $cmd.Path)",
        ],
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )

    assert powershell_lookup.returncode == 0, powershell_lookup.stderr
    assert "ExternalScript|" in powershell_lookup.stdout
    assert str(ps1_path) in powershell_lookup.stdout

    with pytest.raises(FileNotFoundError):
        subprocess.run(
            ["claude", "--version"],
            env=env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )


@pytest.mark.skipif(os.name != "nt", reason="仅在 Windows 上验证 PowerShell 脚本包装逻辑")
def test_resolve_claude_launcher_wraps_powershell_script(monkeypatch, tmp_path: Path):
    ps1_path = tmp_path / "claude.ps1"
    ps1_path.write_text("Write-Output 'fake claude'\n", encoding="utf-8")

    def fake_which(name: str):
        mapping = {
            "claude.cmd": None,
            "claude.exe": None,
            "claude.bat": None,
            "claude": None,
            "pwsh": "C:/Program Files/PowerShell/7/pwsh.exe",
        }
        return mapping.get(name)

    def fake_run(command, **kwargs):
        assert command[:3] == ["C:/Program Files/PowerShell/7/pwsh.exe", "-NoProfile", "-Command"]
        return subprocess.CompletedProcess(command, 0, stdout=str(ps1_path), stderr="")

    monkeypatch.setattr(shutil, "which", fake_which)
    monkeypatch.setattr(claude_cli.subprocess, "run", fake_run)

    launcher = claude_cli._resolve_claude_launcher()

    assert launcher == [
        "C:/Program Files/PowerShell/7/pwsh.exe",
        "-NoProfile",
        "-File",
        str(ps1_path),
    ]
