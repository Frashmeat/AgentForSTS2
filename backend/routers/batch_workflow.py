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
from contextlib import suppress
from pathlib import Path

_log = logging.getLogger("batch")

from fastapi import APIRouter, HTTPException, WebSocket, WebSocketDisconnect

from app.modules.approval.application.action_prompt import build_action_prompt
from app.modules.approval.runtime import get_approval_service
from app.modules.codegen.api import (
    build_and_fix,
    create_asset,
    create_asset_group,
    create_custom_code,
    create_mod_project,
)
from app.modules.planning.api import (
    PlanItem,
    build_execution_plan,
    find_groups,
    plan_from_dict,
    plan_mod,
    topological_sort,
    validate_plan,
)
from app.modules.workflow.application.batch_asset import BatchAssetWorkflow
from app.modules.workflow.application.context import WorkflowContext
from app.modules.workflow.application.engine import WorkflowEngine
from app.modules.workflow.application.policies import LimitedParallelPolicy
from app.modules.workflow.application.step import WorkflowStep
from app.shared.infra.ws_errors import build_ws_error_payload, send_ws_error
from app.shared.kernel.errors import CLIENT_DISCONNECTED_CODE, USER_CANCELLED_CODE, WorkflowTermination
from app.shared.prompting import PromptLoader
from config import get_config
from image.generator import generate_images
from image.postprocess import process_image
from image.prompt_adapter import adapt_prompt, ImageProvider
from llm.agent_runner import resolve_agent_backend
from llm.text_runner import complete_text
from llm.stream_metadata import build_stream_chunk_payload, resolve_agent_display_model
from llm.stage_events import build_stage_event

router = APIRouter()
_TEXT_LOADER = PromptLoader()

TRANSPARENT_TYPES = {"relic", "power"}
def _needs_transparent(asset_type: str) -> bool:
    return asset_type in TRANSPARENT_TYPES


def _img_provider_to_adapter(provider: str) -> ImageProvider:
    mapping = {"bfl": "flux2", "fal": "flux2", "volcengine": "jimeng", "wanxiang": "wanxiang"}
    return mapping.get(provider, "flux2")


def _text(key: str, **variables: object) -> str:
    if variables:
        return _TEXT_LOADER.render(f"runtime_workflow.{key}", variables)
    return _TEXT_LOADER.load(f"runtime_workflow.{key}")


def _normalize_review_strictness(value: object) -> str:
    if value in {"efficient", "balanced", "strict"}:
        return str(value)
    return "balanced"


def _normalize_bundle_decisions(value: object) -> dict[str, str]:
    if not isinstance(value, dict):
        return {}
    normalized: dict[str, str] = {}
    for key, decision in value.items():
        if not isinstance(key, str):
            continue
        if decision in {"unresolved", "accepted", "split_requested", "needs_item_revision"}:
            normalized[key] = str(decision)
    return normalized


def _build_plan_review_payload(plan, strictness: str = "balanced", bundle_decisions: dict[str, str] | None = None) -> dict:
    normalized = _normalize_review_strictness(strictness)
    validation = validate_plan(plan, normalized).to_dict()
    execution_plan = build_execution_plan(plan, normalized, _normalize_bundle_decisions(bundle_decisions)).to_dict()
    return {
        "strictness": normalized,
        "validation": validation,
        "execution_plan": execution_plan,
    }


def _ensure_plan_review_passes(plan, strictness: str = "balanced", bundle_decisions: dict[str, str] | None = None) -> dict:
    normalized_decisions = _normalize_bundle_decisions(bundle_decisions)
    review = _build_plan_review_payload(plan, strictness, normalized_decisions)
    validation_items = review["validation"]["items"]
    bundle_items = review["execution_plan"]["execution_bundles"]

    if any(item["status"] == "invalid" for item in validation_items):
        raise HTTPException(status_code=400, detail="计划存在错误，无法进入执行")
    if any(item["status"] == "needs_user_input" for item in validation_items):
        raise HTTPException(status_code=400, detail="计划仍需补充说明，无法进入执行")
    unresolved_bundles = [
        bundle
        for bundle in bundle_items
        if bundle["status"] != "clear" and normalized_decisions.get(bundle.get("bundle_id", "")) != "accepted"
    ]
    if unresolved_bundles:
        raise HTTPException(status_code=400, detail="执行策略分组仍需确认，无法进入执行")

    return review


async def _run_postprocess(img, asset_type, asset_name, project_root):
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(None, process_image, img, asset_type, asset_name, project_root)


async def _init_batch_project(ws: WebSocket, project_root: Path, send, send_stage) -> Path:
    """缺失 *.csproj 时通过 LLM clone 出一个新 mod 工程；返回（可能更新过的）project_root。"""
    if list(project_root.glob("*.csproj")):
        return project_root

    project_name = project_root.name
    parent_dir = project_root.parent
    await send_stage(
        "project", "project_init",
        _text("batch_project_init_stage", project_name=project_name).strip(),
    )
    await send("batch_progress", message=_text("batch_project_init_progress", project_name=project_name).strip())

    async def _init_stream(chunk: str):
        await send("batch_progress", message=chunk)

    project_root = await create_mod_project(project_name, parent_dir, _init_stream)
    await send("batch_progress", message=_text("batch_project_init_done", project_root=project_root).strip())
    return project_root


async def _obtain_batch_plan(
    ws: WebSocket,
    params: dict,
    *,
    send,
    send_stage,
):
    """根据 action 取得执行计划：start_with_plan 直接读 params；start 走规划 + 等用户确认。"""
    review_strictness = _normalize_review_strictness(params.get("review_strictness"))
    bundle_decisions = _normalize_bundle_decisions(params.get("bundle_decisions"))
    review_gate_requested = "review_strictness" in params

    action = params.get("action")
    if action == "start_with_plan":
        plan = plan_from_dict(params["plan"])
        if review_gate_requested:
            _ensure_plan_review_passes(plan, review_strictness, bundle_decisions)
        return plan

    assert action == "start", _text("batch_start_action_expected").strip()
    requirements: str = params["requirements"]

    await send_stage("text", "planning", _text("batch_planning_stage").strip())
    await send("planning")
    plan = await plan_mod(requirements)
    await send(
        "plan_ready",
        plan=plan.to_dict(),
        review=_build_plan_review_payload(plan, review_strictness, bundle_decisions),
    )

    raw = await ws.receive_text()
    confirm = json.loads(raw)
    assert confirm.get("action") == "confirm_plan", _text("batch_confirm_plan_expected").strip()
    if "review_strictness" in confirm:
        review_gate_requested = True
        review_strictness = _normalize_review_strictness(confirm.get("review_strictness", review_strictness))
    bundle_decisions = _normalize_bundle_decisions(confirm.get("bundle_decisions"))
    if confirm.get("plan"):
        plan = plan_from_dict(confirm["plan"])
    if review_gate_requested:
        _ensure_plan_review_passes(plan, review_strictness, bundle_decisions)
    return plan


async def _generate_item_images(
    item: PlanItem,
    *,
    project_root: Path,
    img_provider: ImageProvider,
    image_gen_sem: asyncio.Semaphore,
    selection_futures: dict,
    send,
    send_stage,
) -> list[Path]:
    """单个 item 的图片阶段：上传图直通；否则 prompt adapt + 生成-选择循环 + 后处理。返回处理后的图片路径列表。"""
    if not item.needs_image:
        return []

    if item.provided_image_b64:
        await send("item_progress", item_id=item.id, message=_text("batch_provided_image_progress").strip())
        from PIL import Image as PilImage
        img_data = base64.b64decode(item.provided_image_b64)
        selected_img = PilImage.open(io.BytesIO(img_data)).convert("RGBA")
    else:
        selected_img = await _run_image_selection_loop(
            item=item,
            img_provider=img_provider,
            image_gen_sem=image_gen_sem,
            selection_futures=selection_futures,
            send=send,
            send_stage=send_stage,
        )

    await send_stage("image", "postprocess", _text("batch_image_postprocess_stage").strip(), item.id)
    await send("item_progress", item_id=item.id, message=_text("batch_image_postprocess_progress").strip())
    paths = await _run_postprocess(selected_img, item.type, item.name, project_root)
    await send("item_progress", item_id=item.id, message=_text("batch_image_postprocess_done").strip())
    return paths


async def _run_image_selection_loop(
    *,
    item: PlanItem,
    img_provider: ImageProvider,
    image_gen_sem: asyncio.Semaphore,
    selection_futures: dict,
    send,
    send_stage,
):
    """prompt adapt → 多轮"生成 → 用户选择 / 修改 prompt 重生" 循环；返回选中的 PIL Image。"""
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
            await send_stage(
                "image", "image_generating",
                _text("batch_image_generating_stage", image_number=image_number).strip(),
                item.id,
            )
            await send(
                "item_progress",
                item_id=item.id,
                message=_text("batch_image_generating_progress", image_number=image_number).strip(),
            )

            async def _img_progress(msg: str, _id=item.id):
                await send("item_progress", item_id=_id, message=msg)

            async def _notify_retry(retry_number: int, _id=item.id):
                await send(
                    "item_progress",
                    item_id=_id,
                    message=_text("batch_image_generating_retry", retry_number=retry_number).strip(),
                )

            img = await _generate_image_with_retry(
                item=item,
                prompt=current_prompt,
                negative_prompt=current_neg,
                image_progress=_img_progress,
                notify_retry=_notify_retry,
            )
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
            return all_images[result["index"]]
        if result.get("prompt"):
            current_prompt = result["prompt"]
        if result.get("negative_prompt") is not None:
            current_neg = result["negative_prompt"]


async def _generate_group_code(
    group: list[PlanItem],
    *,
    item_image_paths: dict[str, list[Path]],
    project_root: Path,
    stream,
) -> None:
    """根据 group 大小分流到 create_asset / create_custom_code / create_asset_group。纯生成动作，不发 ws 事件。"""
    if len(group) == 1:
        item = group[0]
        if item.needs_image:
            await create_asset(
                item.description, item.type, item.name,
                item_image_paths[item.id], project_root, stream,
                name_zhs=item.name_zhs,
                skip_build=True,
            )
        else:
            await create_custom_code(
                item.description, item.implementation_notes,
                item.name, project_root, stream,
                skip_build=True,
            )
        return

    # 多资产合并生成
    assets_spec = [
        {"item": it, "image_paths": item_image_paths.get(it.id, [])}
        for it in group
    ]
    await create_asset_group(assets_spec, project_root, stream)


async def _resolve_group_approval(
    *,
    group,
    group_key,
    approval_states: dict,
    cfg: dict,
    project_root: Path,
    ws: WebSocket,
    replay_deferred,
    execute_actions,
) -> bool:
    """approval_first 分支：推进审批状态机。返回 True 表示可继续到代码生成，False 表示需要等审批/恢复。"""
    state = approval_states.get(group_key)
    if state is None:
        # 第一次进入：生成审批计划 → 发 pending → 重放被搁置的控制消息
        summary, actions = await _plan_group_approval_requests(group, cfg["llm"], project_root)
        approval_states[group_key] = {
            "summary": summary,
            "actions": actions,
            "approved": False,
            "actions_executed": False,
            "resume_requested": False,
        }
        for item in group:
            await _send_item_approval_pending(ws, item.id, summary, actions)
        await replay_deferred(group)
        return False

    if not state["approved"]:
        return False

    if not state["actions_executed"]:
        await execute_actions(group)
    return True


async def _generate_image_with_retry(
    *,
    item,
    prompt: str,
    negative_prompt,
    image_progress,
    notify_retry,
    max_attempts: int = 3,
):
    """生成 1 张图，最多重试 max_attempts 次；CancelledError 透传，最后一次失败后重抛原异常。"""
    for attempt in range(max_attempts):
        try:
            [img] = await generate_images(
                prompt, item.type, negative_prompt,
                batch_size=1, progress_callback=image_progress,
            )
            return img
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            _log.warning(
                "image gen attempt %s failed for item %s: %s",
                attempt + 1, item.id, exc,
            )
            if attempt == max_attempts - 1:
                raise
            await notify_retry(attempt + 2)
            await asyncio.sleep(2)
    # max_attempts >= 1 时上面 for 至少 raise 一次或 return；保险兜底
    raise RuntimeError("unreachable: image retry loop exited without result")


async def _send_item_approval_pending(ws: WebSocket, item_id: str, summary: str, requests: list):
    await ws.send_text(json.dumps({
        "event": "item_approval_pending",
        "item_id": item_id,
        "summary": summary,
        "requests": [request.to_dict() for request in requests],
    }))


async def _publish_batch_standard_event(ws: WebSocket, event) -> None:
    if event.stage == "error":
        payload = build_ws_error_payload(
            code=str(event.payload.get("code", "item_workflow_error")),
            message=event.payload.get("message"),
            detail=event.payload.get("detail"),
            traceback=event.payload.get("traceback"),
            fallback_message=_text("batch_approval_error_default").strip(),
            extra={"item_id": event.payload.get("item_id")} if event.payload.get("item_id") else None,
        )
        await ws.send_text(json.dumps({"event": "item_error", **payload}))
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
    return "modular"


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
        source_backend=resolve_agent_backend(llm_cfg),
        source_workflow="batch",
    )
    return summary, actions


def _group_key(group: list[PlanItem]) -> tuple[str, ...]:
    return tuple(item.id for item in group)


# ── HTTP 端点：规划 ────────────────────────────────────────────────────────────

@router.post("/plan")
def api_plan(body: dict):
    return _api_plan_impl(body)


@router.post("/plan/review")
def api_plan_review(body: dict):
    plan_data = body.get("plan")
    if not isinstance(plan_data, dict):
        raise HTTPException(status_code=400, detail="缺少计划数据，无法评审")
    strictness = _normalize_review_strictness(body.get("strictness"))
    bundle_decisions = _normalize_bundle_decisions(body.get("bundle_decisions"))
    plan = plan_from_dict(plan_data)
    return _build_plan_review_payload(plan, strictness, bundle_decisions)


def _api_plan_impl(body: dict):
    """接收用户需求文本，返回结构化 Mod 计划（JSON）。"""
    requirements: str = body.get("requirements", "")
    if not requirements.strip():
        raise HTTPException(status_code=400, detail=_text("batch_api_requirements_missing").strip())
    plan = asyncio.run(plan_mod(requirements))
    return plan.to_dict()


# ── WebSocket：批量执行 ────────────────────────────────────────────────────────

@router.websocket("/ws/batch")
async def ws_batch(ws: WebSocket):
    await _handle_ws_batch(ws)


async def _handle_ws_batch(ws: WebSocket, *, initial_params: dict | None = None):
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
    if initial_params is None:
        await ws.accept()

    selection_futures: dict[str, asyncio.Future] = {}
    cfg = get_config()
    _log.info("batch workflow mode=%s", _batch_workflow_mode(cfg))
    concurrency = int(cfg.get("image_gen", {}).get("concurrency", 1))
    image_gen_sem = asyncio.Semaphore(max(1, concurrency))
    code_gen_lock = asyncio.Lock()
    item_done_events: dict[str, asyncio.Event] = {}
    approval_states: dict[tuple[str, ...], dict[str, object]] = {}
    deferred_group_messages: dict[str, list[dict]] = {}
    tasks: list[asyncio.Task] = []

    async def send(event: str, **data):
        await ws.send_text(json.dumps({"event": event, **data}))

    async def send_stage(scope: str, stage: str, message: str, item_id: str | None = None):
        payload = build_stage_event(scope, stage, message, item_id=item_id)
        if payload:
            await send("stage_update", **payload)

    async def cancel_pending_tasks() -> None:
        for task in tasks:
            if not task.done():
                task.cancel()
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)

    try:
        # ── 1. 接收启动参数 ──────────────────────────────────────────────────
        if initial_params is None:
            raw = await ws.receive_text()
            params = json.loads(raw)
        else:
            params = initial_params

        project_root = Path(params["project_root"])
        cfg_loaded = get_config()
        agent_display_model = resolve_agent_display_model(cfg_loaded.get("llm", {}))
        img_provider = _img_provider_to_adapter(cfg_loaded["image_gen"]["provider"])

        # ── 2-3. 取得执行计划（含 review gate / start_with_plan / 等用户确认）
        plan = await _obtain_batch_plan(ws, params, send=send, send_stage=send_stage)

        sorted_items = topological_sort(plan.items)
        groups = find_groups(sorted_items)          # 按依赖关系分组
        item_done_events = {item.id: asyncio.Event() for item in sorted_items}
        item_image_events: dict[str, asyncio.Event] = {item.id: asyncio.Event() for item in sorted_items}
        item_image_paths: dict[str, list[Path]] = {}
        error_ids: set[str] = set()

        # ── 4. 检查/初始化项目 ────────────────────────────────────────────────
        project_root = await _init_batch_project(ws, project_root, send, send_stage)

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
                if action.requires_approval and action.status not in {"approved", "succeeded"}:
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
                item_image_paths[item.id] = await _generate_item_images(
                    item,
                    project_root=project_root,
                    img_provider=img_provider,
                    image_gen_sem=image_gen_sem,
                    selection_futures=selection_futures,
                    send=send,
                    send_stage=send_stage,
                )
            except asyncio.CancelledError:
                raise
            except Exception as e:
                _log.exception("item %s image stage failed", item.id)
                try:
                    await send_ws_error(
                        ws,
                        event="item_error",
                        code="item_image_generation_failed",
                        message=str(e),
                        detail=str(e),
                        traceback=tb_module.format_exc(),
                        extra={"item_id": item.id},
                    )
                except (WebSocketDisconnect, RuntimeError) as send_err:
                    # WebSocket 已断开是预期路径；其余 RuntimeError（loop closed 等）也只能放弃推送
                    _log.warning("item %s image error notify skipped: %s", item.id, send_err)
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
                    await send(
                        "item_agent_stream",
                        item_id=first_id,
                        **build_stream_chunk_payload(
                            chunk,
                            source="agent",
                            model=agent_display_model,
                        ),
                    )

                approval_pending = False
                try:
                    if cfg_loaded["llm"].get("execution_mode") == "approval_first":
                        can_continue = await _resolve_group_approval(
                            group=group,
                            group_key=group_key,
                            approval_states=approval_states,
                            cfg=cfg_loaded,
                            project_root=project_root,
                            ws=ws,
                            replay_deferred=replay_deferred_group_messages,
                            execute_actions=execute_group_actions,
                        )
                        if not can_continue:
                            approval_pending = True
                            return

                    await _generate_group_code(
                        group,
                        item_image_paths=item_image_paths,
                        project_root=project_root,
                        stream=_stream,
                    )

                    for item in group:
                        await send("item_done", item_id=item.id, success=True)

                except asyncio.CancelledError:
                    raise
                except Exception as e:
                    tb = tb_module.format_exc()
                    _log.exception("group %s codegen failed", [it.id for it in group])
                    for item in group:
                        try:
                            await send_ws_error(
                                ws,
                                event="item_error",
                                code="item_codegen_failed",
                                message=str(e),
                                detail=str(e),
                                traceback=tb,
                                extra={"item_id": item.id},
                            )
                        except (WebSocketDisconnect, RuntimeError) as send_err:
                            _log.warning("item %s codegen error notify skipped: %s", item.id, send_err)
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

            if action == "cancel":
                raise WorkflowTermination(code=USER_CANCELLED_CODE, message="用户已取消当前批量生成")

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

    except WorkflowTermination as e:
        await cancel_pending_tasks()
        _log.info("batch workflow terminated code=%s", e.code)
        if e.code == USER_CANCELLED_CODE:
            with suppress(Exception):
                await send_ws_error(ws, code=e.code, message=e.message, detail=e.message, event="cancelled")
    except WebSocketDisconnect:
        await cancel_pending_tasks()
        _log.info("batch workflow client disconnected")
    except Exception as e:
        await cancel_pending_tasks()
        try:
            await send_ws_error(
                ws,
                code="batch_workflow_failed",
                message=str(e),
                detail=str(e),
                traceback=tb_module.format_exc(),
            )
        except Exception:
            pass
