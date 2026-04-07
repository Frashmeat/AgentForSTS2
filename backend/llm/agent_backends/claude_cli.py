from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

from ._runner import run_streaming


def _extract_text(event: dict) -> str:
    if event.get("type") == "assistant" and event.get("message"):
        msg = event["message"]
        parts = []
        for block in msg.get("content", []):
            if not isinstance(block, dict):
                continue
            if block.get("type") == "text":
                parts.append(block["text"])
            elif block.get("type") == "tool_use":
                name = block.get("name", "Tool")
                inp = block.get("input", {})
                detail = (
                    inp.get("command")
                    or inp.get("file_path")
                    or inp.get("pattern")
                    or inp.get("prompt")
                    or ""
                )
                parts.append(f"[{name}] {detail}\n" if detail else f"[{name}]\n")
        return "".join(parts)
    if event.get("type") == "result":
        return event.get("result", "")
    return ""


def _process_line(line: str) -> str | None:
    try:
        return _extract_text(json.loads(line)) or None
    except json.JSONDecodeError:
        return line


def _resolve_claude_launcher() -> list[str]:
    if os.name != "nt":
        claude_exe = shutil.which("claude")
        if claude_exe:
            return [claude_exe]
        raise RuntimeError("未找到 Claude CLI，请先安装并确保 claude 可执行文件在 PATH 中")

    for candidate in ("claude.cmd", "claude.exe", "claude.bat", "claude"):
        claude_exe = shutil.which(candidate)
        if claude_exe:
            return [claude_exe]

    script_path = _resolve_claude_powershell_script()
    if script_path:
        powershell_exe = shutil.which("pwsh") or shutil.which("powershell")
        if not powershell_exe:
            raise RuntimeError("已找到 Claude CLI PowerShell 脚本，但未找到可执行的 PowerShell")
        return [powershell_exe, "-NoProfile", "-File", script_path]

    raise RuntimeError("未找到 Claude CLI，请先安装并确保 claude 可执行文件在 PATH 中")


def _resolve_claude_powershell_script() -> str | None:
    powershell_exe = shutil.which("pwsh") or shutil.which("powershell")
    if not powershell_exe:
        return None

    completed = subprocess.run(
        [
            powershell_exe,
            "-NoProfile",
            "-Command",
            "$cmd = Get-Command claude -ErrorAction SilentlyContinue | Select-Object -First 1; if ($cmd -and $cmd.Path) { Write-Output $cmd.Path }",
        ],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if completed.returncode != 0:
        return None

    resolved_path = completed.stdout.strip()
    if not resolved_path:
        return None
    if not resolved_path.lower().endswith(".ps1"):
        return None
    return resolved_path


async def run(prompt: str, project_root: Path, llm_cfg: dict, stream_callback=None) -> str:
    env = os.environ.copy()
    if llm_cfg.get("api_key"):
        env["ANTHROPIC_API_KEY"] = llm_cfg["api_key"]
    if llm_cfg.get("base_url"):
        env["ANTHROPIC_BASE_URL"] = llm_cfg["base_url"]

    cmd = [
        *_resolve_claude_launcher(),
        "--print",
        "--verbose",
        "--dangerously-skip-permissions",
        "--output-format", "stream-json",
        "-p", prompt,
    ]

    output_chunks, _ = await run_streaming(
        cmd,
        cwd=project_root,
        env=env,
        name="Claude CLI",
        process_line=_process_line,
        stream_callback=stream_callback,
    )
    return "".join(output_chunks)
