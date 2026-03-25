from __future__ import annotations

import asyncio
import subprocess

from app.modules.build.application.ports import BuildRequest, BuildResult


class DotnetBuildBackend:
    def __init__(self, runner=None) -> None:
        self._runner = runner or _run_dotnet_command

    async def build(self, request: BuildRequest) -> BuildResult:
        return await self._runner(request.command, request.cwd)


async def _run_dotnet_command(command: list[str], cwd) -> BuildResult:
    loop = asyncio.get_event_loop()
    completed = await loop.run_in_executor(
        None,
        lambda: subprocess.run(
            command,
            cwd=str(cwd),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        ),
    )
    output = (completed.stdout or "") + (completed.stderr or "")
    return BuildResult(
        success=completed.returncode == 0,
        output=output.strip(),
        metadata={"exit_code": completed.returncode},
    )
