from __future__ import annotations

import asyncio
import subprocess
from pathlib import Path

from app.modules.approval.application.ports import ActionResult
from app.modules.build.application.ports import BuildRequest
from app.modules.build.infra.dotnet_builder import DotnetBuildBackend
from approval.models import ActionRequest


class ApprovalExecutor:
    async def execute_action(self, action: ActionRequest) -> ActionResult:
        raise NotImplementedError("ApprovalExecutor subclasses must implement execute_action")


class LocalApprovalExecutor(ApprovalExecutor):
    def __init__(self, allowed_roots: list[Path], allowed_commands: list[list[str]], build_backend=None):
        self.allowed_roots = [root.resolve() for root in allowed_roots]
        self.allowed_commands = allowed_commands
        self.build_backend = build_backend or DotnetBuildBackend()

    def _resolve_path(self, raw_path: str) -> Path:
        candidate = Path(raw_path)
        if not candidate.is_absolute():
            candidate = self.allowed_roots[0] / candidate
        candidate = candidate.resolve()
        if not any(root == candidate or root in candidate.parents for root in self.allowed_roots):
            raise PermissionError(f"path outside allowed roots: {candidate}")
        return candidate

    def _ensure_command_allowed(self, command: list[str]) -> None:
        if not self.allowed_commands:
            return
        if not any(command[:len(prefix)] == prefix for prefix in self.allowed_commands):
            raise PermissionError(f"command not allowed: {' '.join(command)}")

    async def execute_action(self, action: ActionRequest) -> ActionResult:
        if action.kind == "read_file":
            path = self._resolve_path(str(action.payload["path"]))
            return ActionResult(success=True, output=path.read_text(encoding="utf-8"))

        if action.kind == "write_file":
            path = self._resolve_path(str(action.payload["path"]))
            path.parent.mkdir(parents=True, exist_ok=True)
            content = str(action.payload.get("content", ""))
            path.write_text(content, encoding="utf-8")
            return ActionResult(success=True, output="", metadata={"path": str(path)})

        if action.kind == "build_project":
            command = [str(part) for part in action.payload["command"]]
            self._ensure_command_allowed(command)
            cwd_value = str(action.payload.get("cwd", "."))
            cwd = self._resolve_path(cwd_value)
            result = await self.build_backend.build(BuildRequest(command=command, cwd=cwd))
            if not result.success:
                raise RuntimeError(result.output.strip() or "build failed")
            return ActionResult(success=True, output=result.output, metadata=result.metadata)

        if action.kind in {"run_command", "deploy_mod"}:
            command = [str(part) for part in action.payload["command"]]
            self._ensure_command_allowed(command)
            cwd_value = str(action.payload.get("cwd", "."))
            cwd = self._resolve_path(cwd_value)

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
            if completed.returncode != 0:
                raise RuntimeError(output.strip() or f"command exited with {completed.returncode}")
            return ActionResult(success=True, output=output.strip(), metadata={"exit_code": completed.returncode})

        raise ValueError(f"unsupported action kind: {action.kind}")
