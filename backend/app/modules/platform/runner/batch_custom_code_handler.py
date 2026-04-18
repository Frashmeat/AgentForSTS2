from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.shared.prompting import PromptLoader

from .text_generate_handler import execute_text_generate_step


TextStepExecutor = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]

_PROMPT_LOADER = PromptLoader()
_BATCH_CUSTOM_CODE_PROMPT_KEY = "runtime_agent.platform_batch_custom_code_server_user"


def _resolve_item_name(input_payload: dict[str, object]) -> str:
    item_name = str(input_payload.get("item_name", "")).strip()
    if not item_name:
        raise ValueError("custom_code server task requires item_name")
    return item_name


def _string_list(value: object) -> list[str]:
    if not isinstance(value, list):
        return []
    items: list[str] = []
    for entry in value:
        text = str(entry).strip()
        if text:
            items.append(text)
    return items


def _render_multiline_list(values: list[str], *, fallback: str = "无") -> str:
    if not values:
        return fallback
    return "\n".join(f"- {value}" for value in values)


def _build_prompt(input_payload: dict[str, object]) -> str:
    item_name = _resolve_item_name(input_payload)
    description = str(input_payload.get("description", "")).strip()
    goal = str(input_payload.get("goal", "")).strip()
    detailed_description = str(input_payload.get("detailed_description", "")).strip()
    implementation_notes = str(input_payload.get("implementation_notes", "")).strip()
    scope_boundary = str(input_payload.get("scope_boundary", "")).strip()
    dependency_reason = str(input_payload.get("dependency_reason", "")).strip()
    acceptance_notes = str(input_payload.get("acceptance_notes", "")).strip()
    coupling_kind = str(input_payload.get("coupling_kind", "")).strip() or "unclear"

    if not any([description, goal, detailed_description, implementation_notes, acceptance_notes]):
        raise ValueError("custom_code server task requires descriptive input")

    return _PROMPT_LOADER.render(
        _BATCH_CUSTOM_CODE_PROMPT_KEY,
        {
            "item_name": item_name,
            "description": description or "无",
            "goal": goal or "无",
            "detailed_description": detailed_description or "无",
            "implementation_notes": implementation_notes or "无",
            "scope_boundary": scope_boundary or "无",
            "dependency_reason": dependency_reason or "无",
            "acceptance_notes": acceptance_notes or "无",
            "coupling_kind": coupling_kind,
            "affected_targets": _render_multiline_list(_string_list(input_payload.get("affected_targets"))),
            "depends_on": _render_multiline_list(_string_list(input_payload.get("depends_on"))),
            "server_project_name": str(input_payload.get("server_project_name", "")).strip() or "无",
            "server_workspace_root": str(input_payload.get("server_workspace_root", "")).strip() or "无",
        },
    )


def _build_summary(full_text: str, item_name: str) -> str:
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
    return f"已生成 {item_name} 的服务器实现方案"


async def execute_batch_custom_code_step(
    request: StepExecutionRequest,
    *,
    text_step_executor: TextStepExecutor | None = None,
) -> dict[str, object]:
    item_name = _resolve_item_name(request.input_payload)
    prompt = _build_prompt(request.input_payload)
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
    payload["text"] = _build_summary(full_text, item_name)
    payload["item_name"] = item_name
    return payload
