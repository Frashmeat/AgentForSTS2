"""
Build & Deploy：通过 Code Agent 运行 dotnet publish（含 Godot .pck 导出），
成功后把产物复制到 STS2 Mods 文件夹。

为什么用 Code Agent 而不是直接 subprocess：
- dotnet publish 只产出 .dll
- .pck 需要 Godot headless export，Code Agent 的 prompt 里有完整流程
"""
from __future__ import annotations

import json
import shutil
from pathlib import Path

from fastapi import APIRouter, WebSocket

from app.modules.codegen.api import build_and_fix
from app.shared.infra.ws_errors import send_ws_error
from app.shared.prompting import PromptLoader
from config import get_config
from llm.stream_metadata import build_stream_chunk_payload, resolve_agent_display_model

router = APIRouter()
_TEXT_LOADER = PromptLoader()

_SKIP_DIRS = {"obj", "ref", ".godot"}


def _build_deploy_facade(ws: WebSocket):
    container = getattr(getattr(ws.app.state, "container", None), "resolve_optional_singleton", None)
    if container is None:
        return None
    flags = getattr(ws.app.state.container, "platform_migration_flags", None)
    if flags is None or not getattr(flags, "platform_service_split_enabled", False):
        return None
    return ws.app.state.container.resolve_optional_singleton("platform.build_deploy_facade_service")


def _find_output_files(project_root: Path) -> list[Path]:
    """在 bin/ 下找最新的 .dll 和 .pck（跳过 obj/ref 中间产物）。"""
    results: dict[str, Path] = {}
    bin_dir = project_root / "bin"
    if not bin_dir.exists():
        return []
    for suffix in (".dll", ".pck"):
        candidates = [
            f for f in bin_dir.rglob(f"*{suffix}")
            if not any(p in _SKIP_DIRS for p in f.relative_to(bin_dir).parts)
        ]
        if candidates:
            results[suffix] = max(candidates, key=lambda f: f.stat().st_mtime)
    return list(results.values())


@router.websocket("/ws/build-deploy")
async def ws_build_deploy(ws: WebSocket):
    facade = _build_deploy_facade(ws)
    if facade is not None:
        await facade.handle_ws_build_deploy(ws)
        return
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
            await ws.send_text(json.dumps({
                "event": "stream",
                **build_stream_chunk_payload(
                    chunk,
                    source="build",
                    model=display_model,
                ),
            }))

        # ── Step 1: Code Agent 构建（dotnet publish + Godot .pck export）──
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

        if sts2_mods and sts2_mods.exists():
            target_dir = sts2_mods / mod_name
            # local.props 配置了 GameDir 时，dotnet publish 会直接输出到 Mods 文件夹
            # 检查是否已经自动部署过去了
            if target_dir.exists():
                existing = [f for f in target_dir.iterdir() if f.suffix in (".dll", ".pck")]
                if existing:
                    file_names = [f.name for f in existing]
                    deployed_to = str(target_dir)
                    await send_chunk(
                        f"\n{_TEXT_LOADER.render('runtime_workflow.build_deployed_via_local_props', {'target_dir': target_dir}).strip()}\n"
                    )
                    for f in existing:
                        await send_chunk(f"{_TEXT_LOADER.render('runtime_workflow.build_file_item', {'file_name': f.name})}\n")

            # 如果 Mods 里没有，尝试从 bin/ 复制
            if not deployed_to:
                output_files = _find_output_files(project_root)
                if output_files:
                    target_dir.mkdir(parents=True, exist_ok=True)
                    await send_chunk(
                        f"\n{_TEXT_LOADER.render('runtime_workflow.build_copying_to_target', {'target_dir': target_dir}).strip()}\n"
                    )
                    for f in output_files:
                        shutil.copy2(f, target_dir / f.name)
                        await send_chunk(f"{_TEXT_LOADER.render('runtime_workflow.build_file_item', {'file_name': f.name})}\n")
                    file_names = [f.name for f in output_files]
                    deployed_to = str(target_dir)
                    await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_deploy_finished').strip()}\n")
                else:
                    await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_output_missing').strip()}\n")
        elif not sts2_mods:
            await send_chunk(f"\n{_TEXT_LOADER.load('runtime_workflow.build_game_path_missing').strip()}\n")

        await ws.send_text(json.dumps({
            "event": "done",
            "success": True,
            "deployed_to": deployed_to,
            "files": file_names,
        }))

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
