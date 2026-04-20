from __future__ import annotations

import json
from pathlib import Path

from app.modules.platform.application.services.server_deploy_target_lock_service import (
    ServerDeployTargetBusyError,
    ServerDeployTargetLockService,
)
from app.modules.platform.application.services.server_deploy_registry_service import ServerDeployRegistryService
from app.modules.codegen.api import build_and_fix
from app.modules.platform.infra.build_output_files import deploy_latest_output_files, find_latest_output_files
from app.shared.infra.ws_errors import send_ws_error
from app.shared.prompting import PromptLoader
from config import get_config
from project_utils import ensure_local_props


class BuildDeployFacadeService:
    def __init__(self) -> None:
        self._text_loader = PromptLoader()
        self._deploy_target_lock_service = ServerDeployTargetLockService()
        self._deploy_registry_service = ServerDeployRegistryService()

    def _find_output_files(self, project_root: Path) -> list[Path]:
        return find_latest_output_files(project_root)

    async def handle_ws_build_deploy(self, ws) -> None:
        await ws.accept()
        try:
            raw = await ws.receive_text()
            params = json.loads(raw)
            project_root = Path(params["project_root"])

            if not project_root.exists():
                message = self._text_loader.render(
                    "runtime_workflow.build_project_root_missing",
                    {"project_root": project_root},
                )
                await send_ws_error(ws, code="project_root_missing", message=message, detail=message)
                return

            cfg = get_config()
            sts2_path_str = cfg.get("sts2_path", "")
            sts2_mods = Path(sts2_path_str) / "Mods" if sts2_path_str else None
            if sts2_mods is not None and not sts2_mods.exists():
                message = self._text_loader.render(
                    "runtime_workflow.build_game_path_invalid",
                    {"target_dir": sts2_mods},
                ).strip()
                await send_ws_error(ws, code="sts2_mods_path_invalid", message=message, detail=message)
                return

            async def send_chunk(chunk: str):
                await ws.send_text(json.dumps({"event": "stream", "chunk": chunk}))

            ensure_local_props(project_root)
            await send_chunk(f"{self._text_loader.load('runtime_workflow.build_agent_build_start').strip()}\n")
            success, _ = await build_and_fix(project_root, stream_callback=send_chunk)

            if not success:
                message = self._text_loader.load("runtime_workflow.build_build_failed").strip()
                await send_ws_error(ws, code="build_failed", message=message, detail=message)
                return

            await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_build_succeeded').strip()}\n")

            mod_name = project_root.name
            deployed_to: str | None = None
            file_names: list[str] = []

            if sts2_mods and sts2_mods.exists():
                target_dir = sts2_mods / mod_name
                try:
                    lock_handle = self._deploy_target_lock_service.acquire_write_lock(
                        project_name=mod_name,
                        job_id=0,
                        job_item_id=0,
                        user_id=0,
                        source_workspace_root=str(project_root),
                    )
                except ServerDeployTargetBusyError as error:
                    await send_ws_error(ws, code="server_deploy_target_busy", message=str(error), detail=str(error))
                    return
                try:
                    deployed = deploy_latest_output_files(project_root, sts2_mods, project_name=mod_name)
                finally:
                    self._deploy_target_lock_service.release_write_lock(lock_handle)
                if deployed.deployed_to:
                    self._deploy_registry_service.write_registration(
                        target_dir=Path(deployed.deployed_to),
                        project_name=mod_name,
                        job_id=0,
                        job_item_id=0,
                        user_id=0,
                        server_project_ref="",
                        source_workspace_root=str(project_root),
                        deployed_to=deployed.deployed_to,
                        entrypoint="legacy.ws.build_deploy",
                        file_names=list(deployed.file_names),
                    )
                    await send_chunk(
                        f"\n{self._text_loader.render('runtime_workflow.build_copying_to_target', {'target_dir': target_dir}).strip()}\n"
                    )
                    for file_name in deployed.file_names:
                        await send_chunk(
                            f"{self._text_loader.render('runtime_workflow.build_file_item', {'file_name': file_name})}\n"
                        )
                    file_names = list(deployed.file_names)
                    deployed_to = deployed.deployed_to
                    await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_deploy_finished').strip()}\n")
                else:
                    await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_output_missing').strip()}\n")
            elif not sts2_mods:
                await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_game_path_missing').strip()}\n")

            await ws.send_text(json.dumps({
                "event": "done",
                "success": True,
                "deployed_to": deployed_to,
                "files": file_names,
            }))
        except Exception as exc:
            try:
                await send_ws_error(
                    ws,
                    code="build_deploy_failed",
                    message=str(exc),
                    detail=str(exc),
                )
            except Exception:
                pass
