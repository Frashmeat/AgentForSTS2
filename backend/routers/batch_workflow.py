"""
批量 Mod 工作流 API。
支持用户输入自由文本需求 → LLM 规划 → 批量创建多个资产。

并发模型：
- 图片生成：最多 2 个并发（image_gen_sem），等待图片选择时不阻塞其他 item
- 代码生成：串行（code_gen_lock）
- 依赖管理：item_done_events，被依赖的 item 完成后其他 item 才能继续
"""
from __future__ import annotations

import asyncio
import base64
import io
import json
import logging
import traceback as tb_module
from pathlib import Path

_log = logging.getLogger("batch")

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from approval.action_prompt import build_action_prompt
from approval.runtime import get_approval_service
from agents.code_agent import (
    build_and_fix,
    create_asset,
    create_asset_group,
    create_custom_code,
    create_mod_project,
)
from agents.planner import plan_mod, plan_from_dict, topological_sort, find_groups, PlanItem
from app.modules.workflow.application.batch_asset import BatchAssetWorkflow
from app.modules.workflow.application.context import WorkflowContext
from app.modules.workflow.application.engine import WorkflowEngine
from app.modules.workflow.application.policies import LimitedParallelPolicy
from app.modules.workflow.application.step import WorkflowStep
from app.shared.infra.feature_flags import resolve_workflow_migration_flags
from app.shared.prompting import PromptLoader
from config import get_config
from image.generator import generate_images
from image.postprocess import process_image
from image.prompt_adapter import adapt_prompt, ImageProvider
from llm.text_runner import complete_text
from llm.stage_events import build_stage_event

router = APIRouter()
_TEXT_LOADER = PromptLoader()

TRANSPARENT_TYPES = {"relic", "power"}


def _batch_router_service(ws: WebSocket):
    container = getattr(getattr(ws.app.state, "container", None), "resolve_optional_singleton", None)
    if container is None:
        return None
    flags = getattr(ws.app.state.container, "platform_migration_flags", None)
    if flags is None or not getattr(flags, "platform_runner_enabled", False):
        return None
    return ws.app.state.container.resolve_optional_singleton("platform.batch_workflow_router_compat_service")


def _needs_transparent(asset_type: str) -> bool:
    return asset_type in TRANSPARENT_TYPES


def _img_provider_to_adapter(provider: str) -> ImageProvider:
    mapping = {"bfl": "flux2", "fal": "flux2", "volcengine": "jimeng", "wanxiang": "wanxiang"}
    return mapping.get(provider, "flux2")


def _text(key: str, **variables: object) -> str:
    if variables:
        return _TEXT_LOADER.render(f"runtime_workflow.{key}", variables)
    return _TEXT_LOADER.load(f"runtime_workflow.{key}")


async def _run_postprocess(img, asset_type, asset_name, project_root):
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(None, process_image, img, asset_type, asset_name, project_root)


async def _send_item_approval_pending(ws: WebSocket, item_id: str, summary: str, requests: list):
    await ws.send_text(json.dumps({
        "event": "item_approval_pending",
        "item_id": item_id,
        "summary": summary,
        "requests": [request.to_dict() for request in requests],
    }))


async def _publish_batch_standard_event(ws: WebSocket, event) -> None:
    if event.stage == "error":
        await ws.send_text(json.dumps({"event": "item_error", "message": event.payload.get("message", _text("batch_approval_error_default").strip())}))
        return

    if event.payload.get("status") != "completed":
        return

    data = event.payload.get("data", {})
    if event.stage == "approval_pending":
        await ws.send_text(json.dumps({"event": "item_approval_pending", **data}))
    elif event.stage == "image_ready":
        await ws.send_text(json.dumps({"event": "item_image_ready", **data}))
    elif event.stage == "agent_stream":
        await ws.send_text(json.dumps({"event": "item_agent_stream", **data}))
    elif event.stage == "done":
        await ws.send_text(json.dumps({"event": "item_done", **data}))


async def _run_batch_asset_engine(
    ws: WebSocket,
    steps: list[WorkflowStep],
    initial: dict | None = None,
    max_concurrency: int = 1,
) -> WorkflowContext:
    workflow = BatchAssetWorkflow(
        engine=WorkflowEngine(
            policy=LimitedParallelPolicy(max_concurrency=max_concurrency),
            publisher=lambda event: _publish_batch_standard_event(ws, event),
        ),
        max_concurrency=max_concurrency,
    )
    return await workflow.run(steps, WorkflowContext(initial or {}))


def _batch_workflow_mode(config: dict | None = None) -> str:
    flags = resolve_workflow_migration_flags(config)
    return "modular" if flags.use_modular_batch_workflow else "legacy"


async def _plan_group_approval_requests(group: list[PlanItem], llm_cfg: dict, project_root: Path):
    requirements = "\n".join(
        f"- [{item.type}] {item.name}: {item.description or item.implementation_notes}"
        for item in group
    )
    prompt = build_action_prompt(requirements)
    raw = await complete_text(prompt, llm_cfg, cwd=project_root)
    plan = json.loads(raw)
    summary = plan.get("summary", _text("batch_approval_summary_default").strip())
    actions = get_approval_service().create_requests_from_plan(
        plan,
        source_backend=llm_cfg.get("agent_backend", "unknown"),
        source_workflow="batch",
    )
    return summary, actions


def _group_key(group: list[PlanItem]) -> tuple[str, ...]:
    return tuple(item.id for item in group)


# ── HTTP 端点：规划 ────────────────────────────────────────────────────────────

@router.post("/plan")
def api_plan(body: dict):
    return _legacy_api_plan(body)


def _legacy_api_plan(body: dict):
    """接收用户需求文本，返回结构化 Mod 计划（JSON）。"""
    requirements: str = body.get("requirements", "")
    if not requirements.strip():
        return {"error": _text("batch_api_requirements_missing").strip()}
    plan = asyncio.run(plan_mod(requirements))
    return plan.to_dict()


# ── WebSocket：批量执行 ────────────────────────────────────────────────────────

@router.websocket("/ws/batch")
async def ws_batch(ws: WebSocket):
    service = _batch_router_service(ws)
    if service is not None:
        await service.handle_ws_batch(ws)
        return
    await _handle_legacy_ws_batch(ws)


async def _handle_legacy_ws_batch(ws: WebSocket):
    """
    批量创建 Mod 资产的 WebSocket 端点。

    协议（客户端 → 服务端）：
      1. {"action":"start", "requirements":"...", "project_root":"..."}
      2. {"action":"confirm_plan", "plan": {...}}         # 用户审阅后确认（可编辑）
      3. {"action":"select_image", "item_id":"...", "index":0}
      4. {"action":"generate_more", "item_id":"...", "prompt":"..."}

    协议（服务端 → 客户端）：
      planning / plan_ready / batch_progress / batch_started /
      item_started / item_progress / item_image_ready / item_agent_stream /
      item_done / item_error / batch_done / error
    """
    await ws.accept()

    selection_futures: dict[str, asyncio.Future] = {}
    cfg = get_config()
    migration_flags = resolve_workflow_migration_flags(cfg)
    _log.info(
        "batch migration mode=%s unified_ws_contract=%s",
        _batch_workflow_mode(cfg),
        migration_flags.use_unified_ws_contract,
    )
    concurrency = int(cfg.get("image_gen", {}).get("concurrency", 1))
    image_gen_sem = asyncio.Semaphore(max(1, concurrency))
    code_gen_lock = asyncio.Lock()
    item_done_events: dict[str, asyncio.Event] = {}
    approval_states: dict[tuple[str, ...], dict[str, object]] = {}
    deferred_group_messages: dict[str, list[dict]] = {}

    async def send(event: str, **data):
        await ws.send_text(json.dumps({"event": event, **data}))

    async def send_stage(scope: str, stage: str, message: str, item_id: str | None = None):
        payload = build_stage_event(scope, stage, message, item_id=item_id)
        if payload:
            await send("stage_update", **payload)

    try:
        # ── 1. 接收启动参数 ──────────────────────────────────────────────────
        raw = await ws.receive_text()
        params = json.loads(raw)
        action = params.get("action")

        project_root = Path(params["project_root"])
        cfg_loaded = get_config()
        img_provider = _img_provider_to_adapter(cfg_loaded["image_gen"]["provider"])

        if action == "start_with_plan":
            # 直接用已有 plan 执行，跳过规划阶段（恢复上次规划用）
            plan = plan_from_dict(params["plan"])
        else:
            assert action == "start", _text("batch_start_action_expected").strip()
            requirements: str = params["requirements"]

            # ── 2. 规划 ──────────────────────────────────────────────────────
            await send_stage("text", "planning", _text("batch_planning_stage").strip())
            await send("planning")
            plan = await plan_mod(requirements)
            await send("plan_ready", plan=plan.to_dict())

            # ── 3. 等待用户确认计划 ───────────────────────────────────────────
            raw = await ws.receive_text()
            confirm = json.loads(raw)
            assert confirm.get("action") == "confirm_plan", _text("batch_confirm_plan_expected").strip()
            if confirm.get("plan"):
                plan = plan_from_dict(confirm["plan"])

        sorted_items = topological_sort(plan.items)
        groups = find_groups(sorted_items)          # 按依赖关系分组
        item_done_events = {item.id: asyncio.Event() for item in sorted_items}
        item_image_events: dict[str, asyncio.Event] = {item.id: asyncio.Event() for item in sorted_items}
        item_image_paths: dict[str, list[Path]] = {}
        error_ids: set[str] = set()

        # ── 4. 检查/初始化项目 ────────────────────────────────────────────────
        if not list(project_root.glob("*.csproj")):
            project_name = project_root.name
            parent_dir = project_root.parent
            await send_stage("project", "project_init", _text("batch_project_init_stage", project_name=project_name).strip())
            await send("batch_progress", message=_text("batch_project_init_progress", project_name=project_name).strip())

            async def _init_stream(chunk: str):
                await send("batch_progress", message=chunk)

            project_root = await create_mod_project(project_name, parent_dir, _init_stream)
            await send("batch_progress", message=_text("batch_project_init_done", project_root=project_root).strip())

        group_by_item = {
            item.id: group
            for group in groups
            for item in group
        }

        async def approve_group_actions(group: list[PlanItem]) -> None:
            state = approval_states.get(_group_key(group))
            if state is None or state.get("approved"):
                return

            service = get_approval_service()
            store = getattr(service, "store", None)
            if store is None:
                raise RuntimeError("approval service store is not configured")

            for action in state["actions"]:
                if action.requires_approval and action.status != "approved":
                    store.approve_request(action.action_id)

            state["approved"] = True

        async def execute_group_actions(group: list[PlanItem]) -> None:
            state = approval_states.get(_group_key(group))
            if state is None or state.get("actions_executed"):
                return

            service = get_approval_service()
            for action in state["actions"]:
                if action.status == "succeeded":
                    continue
                if action.requires_approval and action.status != "approved":
                    raise RuntimeError("approval request must be approved before execution")
                updated = await service.execute_request(action.action_id)
                if updated.status == "failed":
                    raise RuntimeError(updated.error or "approval action execution failed")

            state["actions_executed"] = True

        async def replay_deferred_group_messages(group: list[PlanItem]) -> None:
            queued: list[dict] = []
            for item in group:
                queued.extend(deferred_group_messages.pop(item.id, []))

            for msg in queued:
                await handle_control_message(msg, defer_if_unready=False)
        if any(len(g) > 1 for g in groups):
            multi = [g for g in groups if len(g) > 1]
            await send("batch_progress", message=_text("batch_multi_group_detected", group_count=len(multi)).strip())

        _log.info("batch_started: %d items, %d groups: %s",
                  len(sorted_items), len(groups),
                  [(it.id, it.type) for it in sorted_items])
        await send("batch_started", items=[it.to_dict() for it in sorted_items])

        # ── 5a. 图片阶段协程（每个 item 独立运行）────────────────────────────
        async def process_item_images(item: PlanItem):
            _log.info("[%s] image task started (needs_image=%s)", item.id, item.needs_image)
            await send("item_started", item_id=item.id, name=item.name, type=item.type)
            try:
                if item.needs_image:
                    if item.provided_image_b64:
                        await send("item_progress", item_id=item.id, message=_text("batch_provided_image_progress").strip())
                        from PIL import Image as PilImage
                        img_data = base64.b64decode(item.provided_image_b64)
                        selected_img = PilImage.open(io.BytesIO(img_data)).convert("RGBA")
                    else:
                        img_desc = item.image_description or item.description
                        async with image_gen_sem:
                            await send_stage("text", "prompt_adapting", _text("batch_prompt_adapting_stage").strip(), item.id)
                            await send("item_progress", item_id=item.id, message=_text("batch_prompt_adapting_progress").strip())
                            adapted = await adapt_prompt(
                                img_desc, item.type, img_provider,
                                needs_transparent_bg=_needs_transparent(item.type),
                            )
                        current_prompt = adapted["prompt"]
                        current_neg = adapted.get("negative_prompt")
                        all_images: list = []
                        while True:
                            async with image_gen_sem:
                                idx = len(all_images)
                                image_number = idx + 1
                                await send_stage("image", "image_generating", _text("batch_image_generating_stage", image_number=image_number).strip(), item.id)
                                await send("item_progress", item_id=item.id, message=_text("batch_image_generating_progress", image_number=image_number).strip())
                                async def _img_progress(msg: str, _id=item.id):
                                    await send("item_progress", item_id=_id, message=msg)
                                for _attempt in range(3):
                                    try:
                                        [img] = await generate_images(
                                            current_prompt, item.type, current_neg,
                                            batch_size=1, progress_callback=_img_progress,
                                        )
                                        break
                                    except Exception as _e:
                                        if _attempt == 2:
                                            raise
                                        await send("item_progress", item_id=item.id, message=_text("batch_image_generating_retry", retry_number=_attempt + 2).strip())
                                        await asyncio.sleep(2)
                                all_images.append(img)
                            buf = io.BytesIO()
                            img.save(buf, format="PNG")
                            b64 = base64.b64encode(buf.getvalue()).decode()
                            await send("item_image_ready", item_id=item.id, image=b64, index=idx, prompt=current_prompt)
                            fut: asyncio.Future = asyncio.get_running_loop().create_future()
                            selection_futures[item.id] = fut
                            result = await fut
                            selection_futures.pop(item.id, None)
                            if result["action"] == "select":
                                selected_img = all_images[result["index"]]
                                break
                            if result.get("prompt"):
                                current_prompt = result["prompt"]
                            if result.get("negative_prompt") is not None:
                                current_neg = result["negative_prompt"]

                    await send_stage("image", "postprocess", _text("batch_image_postprocess_stage").strip(), item.id)
                    await send("item_progress", item_id=item.id, message=_text("batch_image_postprocess_progress").strip())
                    paths = await _run_postprocess(selected_img, item.type, item.name, project_root)
                    item_image_paths[item.id] = paths
                    await send("item_progress", item_id=item.id, message=_text("batch_image_postprocess_done").strip())
                else:
                    item_image_paths[item.id] = []
            except Exception as e:
                try:
                    await send("item_error", item_id=item.id, message=str(e), traceback=tb_module.format_exc())
                except Exception:
                    pass
                error_ids.add(item.id)
            finally:
                item_image_events[item.id].set()

        # ── 5b. 代码阶段协程（按组合并，等所有图片就绪后统一生成）────────────
        async def process_group_code(group: list[PlanItem]):
            _log.info("[group %s] code task waiting for images", [it.id for it in group])
            # 等待组内所有图片就绪
            for item in group:
                await item_image_events[item.id].wait()
            _log.info("[group %s] all images ready, proceeding to code gen", [it.id for it in group])

            # 如果组内有失败的 item，跳过代码生成
            failed = [it.id for it in group if it.id in error_ids]
            if failed:
                for item in group:
                    if item.id not in error_ids:
                        await send("item_error", item_id=item.id,
                                   message=_text("batch_group_image_failure_skip", failed_items=failed).strip())
                        error_ids.add(item.id)
                        item_done_events[item.id].set()
                return

            async with code_gen_lock:
                group_key = _group_key(group)
                # 向所有 item 发送代码生成开始的通知
                first_id = group[0].id
                for item in group:
                    await send_stage("agent", "agent_running", _text("batch_agent_running_stage").strip(), item.id)
                    await send("item_progress", item_id=item.id, message=_text("batch_agent_running_progress").strip())

                async def _stream(chunk: str):
                    await send("item_agent_stream", item_id=first_id, chunk=chunk)

                approval_pending = False
                try:
                    if cfg_loaded["llm"].get("execution_mode") == "approval_first":
                        approval_state = approval_states.get(group_key)
                        if approval_state is None:
                            summary, actions = await _plan_group_approval_requests(group, cfg_loaded["llm"], project_root)
                            approval_states[group_key] = {
                                "summary": summary,
                                "actions": actions,
                                "approved": False,
                                "actions_executed": False,
                                "resume_requested": False,
                            }
                            for item in group:
                                await _send_item_approval_pending(ws, item.id, summary, actions)
                            approval_pending = True
                            await replay_deferred_group_messages(group)
                            return

                        if not approval_state["approved"]:
                            approval_pending = True
                            return

                        if not approval_state["actions_executed"]:
                            await execute_group_actions(group)

                    if len(group) == 1:
                        item = group[0]
                        if item.needs_image:
                            await create_asset(
                                item.description, item.type, item.name,
                                item_image_paths[item.id], project_root, _stream,
                                name_zhs=item.name_zhs,
                                skip_build=True,
                            )
                        else:
                            await create_custom_code(
                                item.description, item.implementation_notes,
                                item.name, project_root, _stream,
                                skip_build=True,
                            )
                    else:
                        # 多资产合并生成
                        assets_spec = [
                            {"item": it, "image_paths": item_image_paths.get(it.id, [])}
                            for it in group
                        ]
                        await create_asset_group(assets_spec, project_root, _stream)

                    for item in group:
                        await send("item_done", item_id=item.id, success=True)

                except Exception as e:
                    tb = tb_module.format_exc()
                    for item in group:
                        try:
                            await send("item_error", item_id=item.id, message=str(e), traceback=tb)
                        except Exception:
                            pass
                        error_ids.add(item.id)
                finally:
                    if not approval_pending:
                        for item in group:
                            item_done_events[item.id].set()

        async def retry_group_for_item(item_id: str):
            group = group_by_item[item_id]
            retry_item = items_by_id[item_id]

            for item in group:
                error_ids.discard(item.id)
                item_done_events[item.id] = asyncio.Event()

            rerun_images = retry_item.needs_image and retry_item.id not in item_image_paths
            if rerun_images:
                item_image_events[retry_item.id] = asyncio.Event()
                tasks.append(asyncio.create_task(process_item_images(retry_item)))
            else:
                item_image_events.setdefault(retry_item.id, asyncio.Event()).set()

            tasks.append(asyncio.create_task(process_group_code(group)))

        async def resume_group_for_item(item_id: str):
            group = group_by_item[item_id]
            group_key = _group_key(group)
            state = approval_states.get(group_key)
            if state is None:
                return
            if state.get("resume_requested"):
                return

            state["resume_requested"] = True
            tasks.append(asyncio.create_task(process_group_code(group)))

        async def handle_control_message(msg: dict, *, defer_if_unready: bool = True):
            action = msg.get("action")
            item_id = msg.get("item_id")

            if action == "select_image" and item_id in selection_futures:
                fut = selection_futures.get(item_id)
                if fut and not fut.done():
                    fut.set_result({"action": "select", "index": msg["index"]})
                return

            if action == "generate_more" and item_id in selection_futures:
                fut = selection_futures.get(item_id)
                if fut and not fut.done():
                    fut.set_result({
                        "action": "generate_more",
                        "prompt": msg.get("prompt"),
                        "negative_prompt": msg.get("negative_prompt"),
                    })
                return

            if action == "approve_all" and item_id in items_by_id:
                group = group_by_item[item_id]
                state = approval_states.get(_group_key(group))
                if state is None:
                    if defer_if_unready:
                        deferred_group_messages.setdefault(item_id, []).append(msg)
                    return
                await approve_group_actions(group)
                return

            if action == "resume" and item_id in items_by_id:
                group = group_by_item[item_id]
                state = approval_states.get(_group_key(group))
                if state is None or not state["approved"]:
                    if defer_if_unready:
                        deferred_group_messages.setdefault(item_id, []).append(msg)
                    return
                await resume_group_for_item(item_id)
                return

            if action == "retry_item" and item_id in items_by_id:
                pending.add(item_id)
                await retry_group_for_item(item_id)

        # ── 6. 启动所有任务 ───────────────────────────────────────────────────
        items_by_id = {item.id: item for item in sorted_items}
        tasks = [asyncio.create_task(process_item_images(item)) for item in sorted_items]
        tasks += [asyncio.create_task(process_group_code(group)) for group in groups]

        # ── 7. 消息接收循环（路由 select/generate_more/retry_item 到对应 future）
        pending = set(item.id for item in sorted_items)
        while pending:
            try:
                raw = await asyncio.wait_for(ws.receive_text(), timeout=1.0)
                msg = json.loads(raw)
                await handle_control_message(msg)

            except asyncio.TimeoutError:
                pass

            done_now = {item.id for item in sorted_items if item_done_events[item.id].is_set()}
            pending -= done_now

        # 等待所有 task 真正结束
        await asyncio.gather(*tasks)

        # ── 最终统一编译（所有资产代码写完后只编译一次）────────────────────────
        if len(error_ids) < len(sorted_items) and cfg_loaded["llm"].get("execution_mode") != "approval_first":
            await send_stage("build", "build_running", _text("batch_build_running_stage").strip())
            await send("batch_progress", message=_text("batch_build_running_progress").strip())
            async def _build_stream(chunk: str):
                await send("batch_progress", message=chunk)
            success, _ = await build_and_fix(project_root, _build_stream)
            if success:
                await send("batch_progress", message=_text("batch_build_success_progress").strip())
            else:
                await send("batch_progress", message=_text("batch_build_failure_progress").strip())

        await send("batch_done", success_count=len(sorted_items) - len(error_ids), error_count=len(error_ids))

    except WebSocketDisconnect:
        pass
    except Exception as e:
        try:
            await send("error", message=str(e), traceback=tb_module.format_exc())
        except Exception:
            pass
