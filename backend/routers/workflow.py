"""
主工作流 API：生图 → 后处理 → Code Agent。
通过 WebSocket 推流进度到前端。
"""
from __future__ import annotations

import asyncio
import base64
import io
import json
import logging
import tempfile
from pathlib import Path
from typing import Literal

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from approval.action_prompt import build_action_prompt
from approval.runtime import get_approval_service
from agents.code_agent import create_asset, create_custom_code, build_and_fix, create_mod_project, package_mod
from app.modules.workflow.application.context import WorkflowContext
from app.modules.workflow.application.engine import WorkflowEngine
from app.modules.workflow.application.single_asset import SingleAssetWorkflow
from app.modules.workflow.application.step import WorkflowStep
from app.shared.infra.feature_flags import resolve_workflow_migration_flags
from app.shared.prompting import PromptLoader
from config import get_config
from project_utils import create_project_from_template
from image.generator import generate_images
from image.postprocess import PROFILES, process_image
from image.prompt_adapter import adapt_prompt, ImageProvider
from llm.text_runner import complete_text
from llm.stage_events import build_stage_event

logger = logging.getLogger(__name__)
router = APIRouter()
_TEXT_LOADER = PromptLoader()

AssetType = Literal["card", "card_fullscreen", "relic", "power", "character"]

# 透明背景资产类型
TRANSPARENT_TYPES = {"relic", "power"}
TRANSPARENT_CHARACTER_VARIANTS = {"character_icon", "map_marker"}


def _needs_transparent(asset_type: AssetType) -> bool:
    return asset_type in TRANSPARENT_TYPES


def _img_provider_to_adapter(provider: str) -> ImageProvider:
    mapping = {"bfl": "flux2", "fal": "flux2", "volcengine": "jimeng", "wanxiang": "wanxiang"}
    return mapping.get(provider, "flux2")


async def _send(ws: WebSocket, event: str, data: dict):
    await ws.send_text(json.dumps({"event": event, **data}))


async def _send_stage(ws: WebSocket, scope: str, stage: str, message: str):
    payload = build_stage_event(scope, stage, message)
    if payload:
        await _send(ws, "stage_update", payload)


def _text(key: str, **variables: object) -> str:
    if variables:
        return _TEXT_LOADER.render(f"runtime_workflow.{key}", variables)
    return _TEXT_LOADER.load(f"runtime_workflow.{key}")


async def _send_approval_pending(ws: WebSocket, summary: str, requests: list):
    await _send(ws, "approval_pending", {
        "summary": summary,
        "requests": [request.to_dict() for request in requests],
    })


async def _publish_standard_event(ws: WebSocket, event) -> None:
    if event.stage == "error":
        await _send(ws, "error", {"message": event.payload.get("message", _text("workflow_prompt_preview_error").strip())})
        return

    if event.payload.get("status") != "completed":
        return

    data = event.payload.get("data", {})
    if event.stage == "prompt_preview":
        await _send(ws, "prompt_preview", data)
    elif event.stage == "image_ready":
        await _send(ws, "image_ready", data)
    elif event.stage == "approval_pending":
        await _send(ws, "approval_pending", data)
    elif event.stage == "agent_stream":
        await _send(ws, "agent_stream", data)
    elif event.stage == "done":
        await _send(ws, "done", data)
    elif event.stage == "build_started":
        await _send_stage(ws, "build", "build_started", data.get("message", _text("workflow_build_started").strip()))
    elif event.stage == "build_finished":
        await _send_stage(ws, "build", "build_finished", data.get("message", _text("workflow_build_finished").strip()))


async def _run_single_asset_engine(ws: WebSocket, steps: list[WorkflowStep], initial: dict | None = None) -> WorkflowContext:
    workflow = SingleAssetWorkflow(
        engine=WorkflowEngine(publisher=lambda event: _publish_standard_event(ws, event))
    )
    return await workflow.run(steps, WorkflowContext(initial or {}))


def _single_workflow_mode(config: dict | None = None) -> str:
    flags = resolve_workflow_migration_flags(config)
    return "modular" if flags.use_modular_single_workflow else "legacy"


async def _maybe_await_approval(
    ws: WebSocket,
    description: str,
    llm_cfg: dict,
    project_root: Path,
) -> tuple[bool, str | None]:
    """审批模式下：等待用户确认，并在 approval_first 下返回主执行链。

    返回值:
    - (True, None): 继续默认执行链
    - (False, None): 用户取消
    - (False, output): 保留给兼容分支；当前单资产流程不会使用
    """
    if llm_cfg.get("execution_mode") != "approval_first":
        return True, None
    summary, actions = await _plan_approval_requests(description, llm_cfg, project_root)
    await _send_approval_pending(ws, summary, actions)
    decision = json.loads(await ws.receive_text())
    if decision.get("action") != "approve_all":
        await _send(ws, "done", {"success": False, "image_paths": [], "agent_output": _text("workflow_approval_cancelled_output").strip()})
        return False, None
    await _send_stage(ws, "agent", "agent_running", _text("workflow_approval_passed_stage").strip())
    await _send(ws, "progress", {"message": _text("workflow_approval_passed_progress").strip()})
    return True, None


async def _plan_approval_requests(description: str, llm_cfg: dict, project_root: Path):
    prompt = build_action_prompt(description)
    raw = await complete_text(prompt, llm_cfg, cwd=project_root)
    plan = json.loads(raw)
    service = get_approval_service()
    summary = plan.get("summary", _text("workflow_approval_output_default").strip())
    actions = service.create_requests_from_plan(
        plan,
        source_backend=llm_cfg.get("agent_backend", "unknown"),
        source_workflow="single_asset",
    )
    return summary, actions


@router.websocket("/ws/create")
async def ws_create(ws: WebSocket):
    """
    WebSocket 端点，驱动完整的创建工作流。

    客户端首先发送 JSON：
    {
        "action": "start",
        "asset_type": "card" | "relic" | "power" | "character",
        "asset_name": "DarkBlade",
        "description": "一把暗黑匕首，造成8点伤害...",
        "project_root": "/path/to/mod/project"
    }

    然后等待 image_batch 事件，发送 {"action": "select", "index": 0}

    服务端推流事件：
    - progress: {message}
    - image_batch: {images: [base64, ...]}
    - agent_stream: {chunk}
    - done: {success, output_files}
    - error: {message}
    """
    await ws.accept()
    client = ws.client
    logger.info("[ws/create] 连接建立 client=%s", client)
    try:
        # 1. 接收初始参数
        raw = await ws.receive_text()
        params = json.loads(raw)
        assert params.get("action") == "start"

        asset_type: AssetType = params["asset_type"]
        asset_name: str = params["asset_name"]
        description: str = params["description"]
        project_root = Path(params["project_root"])
        logger.info("[ws/create] asset_type=%s asset_name=%s project=%s", asset_type, asset_name, project_root)

        # custom_code 类型：跳过图片生成，直接走代码 agent
        if asset_type == "custom_code":
            logger.info("[ws/create] 走 custom_code 分支")
            await _ws_run_custom_code(ws, params, project_root)
            return

        # 用户提供了图片（路径或 base64）：跳过生图/选图，直接后处理 + code agent
        if params.get("provided_image_path") or params.get("provided_image_b64"):
            logger.info("[ws/create] 走 provided_image 分支")
            await _ws_run_with_provided_image(ws, params, project_root)
            return

        cfg = get_config()
        migration_flags = resolve_workflow_migration_flags(cfg)
        logger.info(
            "[ws/create] migration mode=%s unified_ws_contract=%s",
            _single_workflow_mode(cfg),
            migration_flags.use_unified_ws_contract,
        )
        # 前端可覆盖 batch_size，否则用 config 默认值
        if "batch_size" in params:
            cfg["image_gen"]["batch_size"] = int(params["batch_size"])
        img_provider = _img_provider_to_adapter(cfg["image_gen"]["provider"])

        # 1.5 检测项目是否存在，不存在则从本地模板（S01_IronStrike）复制
        if not list(project_root.glob("*.csproj")):
            project_name = project_root.name
            parent_dir = project_root.parent
            await _send_stage(ws, "project", "project_init", _text("workflow_project_init_stage", project_name=project_name).strip())
            await _send(ws, "progress", {"message": _text("workflow_project_init_progress", project_name=project_name).strip()})
            try:
                project_root = await asyncio.get_event_loop().run_in_executor(
                    None, create_project_from_template, project_name, parent_dir
                )
            except FileNotFoundError:
                # 模板不存在时回退到 Claude clone 方式
                async def _stream_init(chunk: str):
                    await _send(ws, "agent_stream", {"chunk": chunk})
                project_root = await create_mod_project(project_name, parent_dir, _stream_init)
            await _send(ws, "progress", {"message": _text("workflow_project_init_done", project_root=project_root).strip()})

        # 2. Prompt Adaptation
        await _send_stage(ws, "text", "prompt_adapting", _text("workflow_prompt_adapting_stage").strip())
        await _send(ws, "progress", {"message": _text("workflow_prompt_adapting_progress").strip()})
        adapted = await adapt_prompt(
            description,
            asset_type,
            img_provider,
            needs_transparent_bg=_needs_transparent(asset_type),
        )

        # 2.5 发送 prompt 预览，等待用户确认（可修改 prompt）
        await _send(ws, "prompt_preview", {
            "prompt": adapted["prompt"],
            "negative_prompt": adapted.get("negative_prompt", ""),
            "fallback_warning": adapted.get("fallback_warning"),
        })
        raw = await ws.receive_text()
        confirm = json.loads(raw)
        assert confirm.get("action") == "confirm"
        # 用户可能修改了 prompt
        if confirm.get("prompt"):
            adapted["prompt"] = confirm["prompt"]
        if confirm.get("negative_prompt") is not None:
            adapted["negative_prompt"] = confirm["negative_prompt"]

        # 3. 图像生成循环：每次生成1张，推给前端，等用户决策
        all_images: list = []
        current_prompt = adapted["prompt"]
        current_neg = adapted.get("negative_prompt")

        while True:
            idx = len(all_images)
            image_number = idx + 1
            await _send_stage(ws, "image", "image_generating", _text("workflow_image_generating_stage", image_number=image_number).strip())
            await _send(ws, "progress", {"message": _text("workflow_image_generating_progress", image_number=image_number).strip()})

            async def _img_progress(msg: str):
                await _send(ws, "progress", {"message": msg})

            [img] = await generate_images(current_prompt, asset_type, current_neg, batch_size=1, progress_callback=_img_progress)
            all_images.append(img)

            buf = io.BytesIO()
            img.save(buf, format="PNG")
            b64 = base64.b64encode(buf.getvalue()).decode()
            await _send(ws, "image_ready", {"image": b64, "index": idx, "prompt": current_prompt})

            # 等用户决定：select 或 generate_more
            raw = await ws.receive_text()
            action_data = json.loads(raw)

            if action_data.get("action") == "select":
                selected_img = all_images[action_data["index"]]
                break
            elif action_data.get("action") == "generate_more":
                if action_data.get("prompt"):
                    current_prompt = action_data["prompt"]
                if action_data.get("negative_prompt") is not None:
                    current_neg = action_data["negative_prompt"]
                # 继续循环生成下一张

        # 5. 后处理
        await _send_stage(ws, "image", "postprocess", _text("workflow_image_postprocess_stage").strip())
        await _send(ws, "progress", {"message": _text("workflow_image_postprocess_progress").strip()})
        image_paths = await _run_postprocess(selected_img, asset_type, asset_name, project_root)
        await _send(ws, "progress", {"message": _text("workflow_image_paths_written", image_paths=[str(p) for p in image_paths]).strip()})

        # 6. Code Agent
        should_continue, approval_output = await _maybe_await_approval(ws, description, cfg["llm"], project_root)
        if approval_output is not None:
            await _send(ws, "done", {
                "success": True,
                "image_paths": [str(p) for p in image_paths],
                "agent_output": approval_output,
            })
            return
        if not should_continue:
            return
        if cfg["llm"].get("execution_mode") != "approval_first":
            await _send_stage(ws, "agent", "agent_running", _text("workflow_agent_running_stage").strip())
            await _send(ws, "progress", {"message": _text("workflow_agent_running_progress").strip()})

        async def stream_to_ws(chunk: str):
            await _send(ws, "agent_stream", {"chunk": chunk})

        output = await create_asset(
            description, asset_type, asset_name,
            image_paths, project_root, stream_to_ws,
        )

        # 7. 完成
        await _send(ws, "done", {
            "success": True,
            "image_paths": [str(p) for p in image_paths],
            "agent_output": output,
        })

    except WebSocketDisconnect:
        logger.info("[ws/create] 客户端主动断开 client=%s", client)
    except Exception as e:
        import traceback
        msg = _friendly_error(e)
        tb = traceback.format_exc()
        logger.error("[ws/create] 未捕获异常 client=%s\n%s", client, tb)
        try:
            await _send(ws, "error", {"message": msg, "traceback": tb})
        except Exception:
            pass


async def _ws_run_custom_code(ws: WebSocket, params: dict, project_root: Path):
    """custom_code 类型：跳过图片生成，直接调用 create_custom_code agent。"""
    asset_name: str = params["asset_name"]
    description: str = params["description"]
    implementation_notes: str = params.get("implementation_notes", "")

    # 1.5 建立项目（如果不存在），从本地模板复制
    if not list(project_root.glob("*.csproj")):
        project_name = project_root.name
        parent_dir = project_root.parent
        await _send_stage(ws, "project", "project_init", _text("workflow_project_init_stage", project_name=project_name).strip())
        await _send(ws, "progress", {"message": _text("workflow_project_init_progress", project_name=project_name).strip()})
        try:
            project_root = await asyncio.get_event_loop().run_in_executor(
                None, create_project_from_template, project_name, parent_dir
            )
        except FileNotFoundError:
            async def _stream_init(chunk: str):
                await _send(ws, "agent_stream", {"chunk": chunk})
            project_root = await create_mod_project(project_name, parent_dir, _stream_init)
        await _send(ws, "progress", {"message": _text("workflow_project_init_done", project_root=project_root).strip()})

    cfg = get_config()
    should_continue, approval_output = await _maybe_await_approval(ws, description, cfg["llm"], project_root)
    if approval_output is not None:
        await _send(ws, "done", {
            "success": True,
            "image_paths": [],
            "agent_output": approval_output,
        })
        return
    if not should_continue:
        return
    if cfg["llm"].get("execution_mode") != "approval_first":
        await _send_stage(ws, "agent", "agent_running", _text("workflow_custom_code_agent_running_stage").strip())
        await _send(ws, "progress", {"message": _text("workflow_custom_code_agent_running_progress").strip()})

    async def stream_to_ws(chunk: str):
        await _send(ws, "agent_stream", {"chunk": chunk})

    output = await create_custom_code(
        description=description,
        implementation_notes=implementation_notes,
        name=asset_name,
        project_root=project_root,
        stream_callback=stream_to_ws,
    )

    await _send(ws, "done", {
        "success": True,
        "image_paths": [],
        "agent_output": output,
    })


async def _ws_run_with_provided_image(ws: WebSocket, params: dict, project_root: Path):
    """用户自定义图片（base64 或本地路径）→ 后处理 → code agent，跳过生图和选图步骤。"""
    from PIL import Image as PILImage

    asset_type: AssetType = params["asset_type"]
    asset_name: str = params["asset_name"]
    description: str = params["description"]

    # 优先用 base64（浏览器上传），fallback 到本地路径
    if params.get("provided_image_b64"):
        import base64, io as _io
        raw = base64.b64decode(params["provided_image_b64"])
        img_src = PILImage.open(_io.BytesIO(raw))
        fname = params.get("provided_image_name", "uploaded")
    else:
        image_path = Path(params["provided_image_path"])
        if not image_path.exists():
            await _send(ws, "error", {"message": _text("workflow_provided_image_missing", image_path=image_path).strip()})
            return
        img_src = PILImage.open(image_path)
        fname = image_path.name

    # 初始化项目（如有需要）
    if not list(project_root.glob("*.csproj")):
        project_name = project_root.name
        parent_dir = project_root.parent
        await _send_stage(ws, "project", "project_init", _text("workflow_project_init_stage", project_name=project_name).strip())
        await _send(ws, "progress", {"message": _text("workflow_project_init_progress", project_name=project_name).strip()})
        try:
            project_root = await asyncio.get_event_loop().run_in_executor(
                None, create_project_from_template, project_name, parent_dir
            )
        except FileNotFoundError:
            async def _stream_init(chunk: str):
                await _send(ws, "agent_stream", {"chunk": chunk})
            project_root = await create_mod_project(project_name, parent_dir, _stream_init)
        await _send(ws, "progress", {"message": _text("workflow_project_init_done", project_root=project_root).strip()})

    await _send(ws, "progress", {"message": _text("workflow_provided_image_reading", file_name=fname).strip()})
    img = await asyncio.get_event_loop().run_in_executor(
        None, lambda: img_src.convert("RGBA")
    )

    await _send_stage(ws, "image", "postprocess", _text("workflow_image_postprocess_stage").strip())
    await _send(ws, "progress", {"message": _text("workflow_image_postprocess_progress").strip()})
    image_paths = await _run_postprocess(img, asset_type, asset_name, project_root)
    await _send(ws, "progress", {"message": _text("workflow_image_paths_written", image_paths=[str(p) for p in image_paths]).strip()})

    cfg = get_config()
    should_continue, approval_output = await _maybe_await_approval(ws, description, cfg["llm"], project_root)
    if approval_output is not None:
        await _send(ws, "done", {
            "success": True,
            "image_paths": [str(p) for p in image_paths],
            "agent_output": approval_output,
        })
        return
    if not should_continue:
        return
    if cfg["llm"].get("execution_mode") != "approval_first":
        await _send_stage(ws, "agent", "agent_running", _text("workflow_agent_running_stage").strip())
        await _send(ws, "progress", {"message": _text("workflow_agent_running_progress").strip()})

    async def stream_to_ws(chunk: str):
        await _send(ws, "agent_stream", {"chunk": chunk})

    output = await create_asset(
        description, asset_type, asset_name,
        image_paths, project_root, stream_to_ws,
    )

    await _send(ws, "done", {
        "success": True,
        "image_paths": [str(p) for p in image_paths],
        "agent_output": output,
    })


def _friendly_error(e: Exception) -> str:
    s = str(e)
    if "401" in s:
        return _text("workflow_api_key_invalid").strip()
    if "403" in s:
        return _text("workflow_api_key_forbidden").strip()
    if "getaddrinfo" in s or "ConnectError" in type(e).__name__:
        return _text("workflow_network_error", error_type=type(e).__name__).strip()
    if "timeout" in s.lower() or "Timeout" in type(e).__name__:
        return _text("workflow_timeout_error").strip()
    return s


async def _run_postprocess(img, asset_type, asset_name, project_root):
    """在线程池中执行同步后处理，避免阻塞事件循环。"""
    import asyncio
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(
        None,
        process_image,
        img, asset_type, asset_name, project_root,
    )


# ── 其他 HTTP 端点 ────────────────────────────────────────────────────────────

@router.post("/project/create")
async def api_create_project(body: dict):
    """创建新 mod 项目。"""
    project_name = body["name"]
    target_dir = Path(body["target_dir"])
    project_path = await create_mod_project(project_name, target_dir)
    return {"project_path": str(project_path)}


@router.post("/project/build")
async def api_build(body: dict):
    """手动触发 build。"""
    project_root = Path(body["project_root"])
    success, output = await build_and_fix(project_root)
    return {"success": success, "output": output}


@router.post("/project/package")
async def api_package(body: dict):
    """打包 mod。"""
    project_root = Path(body["project_root"])
    success = await package_mod(project_root)
    return {"success": success}
