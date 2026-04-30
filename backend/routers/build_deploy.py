"""
Build & Deploy：通过 Code Agent 运行 dotnet publish（含 Godot .pck 导出），
成功后把产物复制到 STS2 Mods 文件夹。

为什么用 Code Agent 而不是直接 subprocess：
- dotnet publish 只产出 .dll
- .pck 需要 Godot headless export，Code Agent 的 prompt 里有完整流程
"""

from __future__ import annotations

import json
from pathlib import Path

from fastapi import APIRouter, WebSocket
from project_utils import ensure_local_props

from app.modules.codegen.api import build_and_fix
from app.modules.platform.application.services import (
    ServerDeployRegistryService,
    ServerDeployTargetBusyError,
    ServerDeployTargetLockService,
)
from app.modules.platform.infra.build_output_files import deploy_latest_output_files
from app.shared.infra.ws_errors import send_ws_error
from app.shared.prompting import PromptLoader
from config import get_config
from llm.stream_metadata import build_stream_chunk_payload, resolve_agent_display_model

router = APIRouter()
_TEXT_LOADER = PromptLoader()
_DEPLOY_TARGET_LOCK_SERVICE = ServerDeployTargetLockService()
_DEPLOY_REGISTRY_SERVICE = ServerDeployRegistryService()


@router.websocket("/ws/build-deploy")
async def ws_build_deploy(ws: WebSocket):
    """
    WebSocket：Code Agent 构建 mod，成功后复制产物到 STS2 Mods 文件夹。

    客户端发送：{ "project_root": "E:/STS2mod/MyMod" }

    服务端推流：
    - stream:  { chunk }
    - done:    { success, deployed_to, files }
    - error:   { message }
    """
    await ws.accept()
    try:
        raw = await ws.receive_text()
        params = json.loads(raw)
        project_root = Path(params["project_root"])

        if not project_root.exists():
            message = _TEXT_LOADER.render("runtime_workflow.build_project_root_missing", {"project_root": project_root})
            await send_ws_error(ws, code="project_root_missing", message=message, detail=message)
            return

        cfg = get_config()
        llm_cfg = cfg.get("llm", {})
        display_model = resolve_agent_display_model(llm_cfg)
        sts2_path_str = cfg.get("sts2_path", "")
        sts2_mods = Path(sts2_path_str) / "Mods" if sts2_path_str else None
        if sts2_mods is not None and not sts2_mods.exists():
            message = _TEXT_LOADER.render("runtime_workflow.build_game_path_invalid", {"target_dir": sts2_mods}).strip()
            await send_ws_error(ws, code="sts2_mods_path_invalid", message=message, detail=message)
            return

        async def send_chunk(chunk: str):
            await ws.send_text(
                json.dumps(
                    {
                        "event": "stream",
                        **build_stream_chunk_payload(
                            chunk,
                            source="build",
                            model=display_model,
                        ),
                    }
                )
            )

        # ── Step 1: Code Agent 构建（dotnet publish + Godot .pck export）──
        ensure_local_props(project_root)
        await send_chunk(f"{_TEXT_LOADER.load('runtime_workflow.build_agent_build_start').strip()}\n")
        success, _ = await build_and_fix(project_root, stream_callback=send_chunk)

        if not success:
            message = _TEXT_LOADER.load("runtime_workflow.build_build_failed").strip()
            await send_ws_error(ws, code="build_failed", message=message, detail=message)
            return

        await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_build_succeeded').strip()}\n")

        mod_name = project_root.name
        deployed_to: str | None = None
        file_names: list[str] = []
        last_successful_deploy: dict[str, object] | None = None
        deploy_recovery_context: dict[str, object] | None = None

        if sts2_mods and sts2_mods.exists():
            target_dir = sts2_mods / mod_name
            try:
                lock_handle = _DEPLOY_TARGET_LOCK_SERVICE.acquire_write_lock(
                    project_name=mod_name,
                    job_id=0,
                    job_item_id=0,
                    user_id=0,
                    source_workspace_root=str(project_root),
                )
            except ServerDeployTargetBusyError as error:
                registration = _DEPLOY_REGISTRY_SERVICE.read_registration(target_dir)
                error.last_successful_deploy = _DEPLOY_REGISTRY_SERVICE.build_registration_payload(registration)
                error.recovery_context = _DEPLOY_REGISTRY_SERVICE.build_recovery_context(
                    registration,
                    requested_source_workspace_root=str(project_root),
                )
                await send_ws_error(
                    ws,
                    code="server_deploy_target_busy",
                    message=str(error),
                    detail=str(error),
                    extra={
                        "last_successful_deploy": error.last_successful_deploy,
                        "recovery_context": error.recovery_context,
                    },
                )
                return
            try:
                deployed = deploy_latest_output_files(project_root, sts2_mods, project_name=mod_name)
            finally:
                _DEPLOY_TARGET_LOCK_SERVICE.release_write_lock(lock_handle)

            if deployed.deployed_to:
                _DEPLOY_REGISTRY_SERVICE.write_registration(
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
                last_successful_deploy = _DEPLOY_REGISTRY_SERVICE.build_registration_payload(
                    _DEPLOY_REGISTRY_SERVICE.read_registration(Path(deployed.deployed_to))
                )
                deploy_recovery_context = _DEPLOY_REGISTRY_SERVICE.build_recovery_context(
                    _DEPLOY_REGISTRY_SERVICE.read_registration(Path(deployed.deployed_to)),
                    requested_source_workspace_root=str(project_root),
                )
                await send_chunk(
                    f"\n{_TEXT_LOADER.render('runtime_workflow.build_copying_to_target', {'target_dir': target_dir}).strip()}\n"
                )
                for file_name in deployed.file_names:
                    await send_chunk(
                        f"{_TEXT_LOADER.render('runtime_workflow.build_file_item', {'file_name': file_name})}\n"
                    )
                file_names = list(deployed.file_names)
                deployed_to = deployed.deployed_to
                await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_deploy_finished').strip()}\n")
            else:
                await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_output_missing').strip()}\n")
        elif not sts2_mods:
            await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_game_path_missing').strip()}\n")

        await ws.send_text(
            json.dumps(
                {
                    "event": "done",
                    "success": True,
                    "deployed_to": deployed_to,
                    "files": file_names,
                    "last_successful_deploy": last_successful_deploy,
                    "deploy_recovery_context": deploy_recovery_context,
                }
            )
        )

    except Exception as e:
        try:
            await send_ws_error(
                ws,
                code="build_deploy_failed",
                message=str(e),
                detail=str(e),
            )
        except Exception:
            pass
