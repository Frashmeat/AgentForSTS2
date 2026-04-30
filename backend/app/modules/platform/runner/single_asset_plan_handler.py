from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.modules.knowledge.infra.sts2_code_facts_provider import Sts2CodeFactsProvider
from app.modules.knowledge.infra.sts2_guidance_provider import Sts2GuidanceProvider
from app.modules.knowledge.infra.sts2_knowledge_resolver import Sts2KnowledgeResolver
from app.modules.knowledge.infra.sts2_lookup_provider import Sts2LookupProvider
from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.shared.contracts.knowledge import KnowledgeQuery
from app.shared.prompting import PromptContextAssembler, PromptLoader

from .server_workspace_snapshot import render_server_workspace_snapshot
from .text_generate_handler import execute_text_generate_step

TextStepExecutor = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]

_PROMPT_LOADER = PromptLoader()
_PROMPT_CONTEXT_ASSEMBLER = PromptContextAssembler()
_KNOWLEDGE_RESOLVER = Sts2KnowledgeResolver(
    code_facts_provider=Sts2CodeFactsProvider(),
    guidance_provider=Sts2GuidanceProvider(),
    lookup_provider=Sts2LookupProvider(),
)
_SINGLE_ASSET_PLAN_PROMPT_KEY = "runtime_agent.platform_single_asset_server_user"
_ASSET_TYPE_LABELS = {
    "card": "卡牌",
    "card_fullscreen": "全画面卡",
    "relic": "遗物",
    "power": "能力图标",
    "character": "角色",
}


def _resolve_asset_type(input_payload: dict[str, object]) -> str:
    asset_type = str(input_payload.get("asset_type", "")).strip()
    return asset_type or "unknown"


def _resolve_item_name(input_payload: dict[str, object]) -> str:
    item_name = str(input_payload.get("item_name", "")).strip()
    if not item_name:
        raise ValueError("single asset server task requires item_name")
    return item_name


def _build_prompt(input_payload: dict[str, object]) -> tuple[str, str, str]:
    asset_type = _resolve_asset_type(input_payload)
    item_name = _resolve_item_name(input_payload)
    description = str(input_payload.get("description", "")).strip()
    if not description:
        raise ValueError("single asset server task requires description")

    packet = _KNOWLEDGE_RESOLVER.resolve(
        KnowledgeQuery(
            scenario="asset_codegen",
            domain="sts2",
            asset_type=asset_type,
            requirements=description,
            item_name=item_name,
        )
    )
    knowledge = _PROMPT_CONTEXT_ASSEMBLER.assemble(packet)
    asset_type_label = _ASSET_TYPE_LABELS.get(asset_type, asset_type or "资产")
    prompt = _PROMPT_LOADER.render(
        _SINGLE_ASSET_PLAN_PROMPT_KEY,
        {
            "asset_type": asset_type,
            "asset_type_label": asset_type_label,
            "item_name": item_name,
            "description": description,
            "image_mode": str(input_payload.get("image_mode", "")).strip() or "ai",
            "has_uploaded_image": "是" if str(input_payload.get("uploaded_asset_ref", "")).strip() else "否",
            "uploaded_asset_file_name": str(input_payload.get("uploaded_asset_file_name", "")).strip() or "无",
            "uploaded_asset_mime_type": str(input_payload.get("uploaded_asset_mime_type", "")).strip() or "无",
            "uploaded_asset_size_bytes": str(input_payload.get("uploaded_asset_size_bytes", "")).strip() or "无",
            "server_project_name": str(input_payload.get("server_project_name", "")).strip() or "无",
            "server_workspace_root": str(input_payload.get("server_workspace_root", "")).strip() or "无",
            "server_workspace_snapshot": render_server_workspace_snapshot(input_payload.get("server_workspace_root")),
            "facts": knowledge["facts"] or "无",
            "guidance": knowledge["guidance"] or "无",
            "lookup": knowledge["lookup"] or "无",
            "knowledge_warnings": knowledge["knowledge_warnings"],
        },
    )
    return prompt, asset_type, item_name


def _build_summary(full_text: str, asset_type: str, item_name: str) -> str:
    for raw_line in full_text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("摘要："):
            line = line[3:].strip()
        elif line.lower().startswith("summary:"):
            line = line.split(":", 1)[1].strip()
        if line:
            return line[:120]
    asset_type_label = _ASSET_TYPE_LABELS.get(asset_type, item_name)
    return f"已生成服务器{asset_type_label}实现方案"


async def execute_single_asset_plan_step(
    request: StepExecutionRequest,
    *,
    text_step_executor: TextStepExecutor | None = None,
) -> dict[str, object]:
    prompt, asset_type, item_name = _build_prompt(request.input_payload)
    if text_step_executor is None:
        text_step_executor = execute_text_generate_step

    forwarded_request = StepExecutionRequest(
        workflow_version=request.workflow_version,
        step_protocol_version=request.step_protocol_version,
        step_type="text.generate",
        step_id=f"{request.step_id}.text",
        job_id=request.job_id,
        job_item_id=request.job_item_id,
        result_schema_version=request.result_schema_version,
        input_payload={"prompt": prompt},
        execution_binding=request.execution_binding,
    )
    result = await text_step_executor(forwarded_request)
    full_text = str(result.get("text", "")).strip()
    payload = dict(result)
    payload["analysis"] = full_text
    payload["text"] = _build_summary(full_text, asset_type, item_name)
    payload["asset_type"] = asset_type
    payload["item_name"] = item_name
    return payload
