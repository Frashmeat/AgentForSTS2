from __future__ import annotations

from collections.abc import Awaitable, Callable
from pathlib import Path

from app.modules.codegen.api import build_custom_code_prompt
from app.modules.codegen.domain.models import CustomCodegenRequest
from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from llm.agent_runner import run_agent_task_with_llm_config


CodeAgentRunner = Callable[[str, Path, dict[str, object]], Awaitable[str]]


def build_code_llm_config(binding: StepExecutionBinding) -> dict[str, object]:
    model = str(binding.model).strip()
    credential = str(binding.credential).strip()
    if not model:
        raise ValueError("execution_binding.model is required")
    if not credential:
        raise ValueError("execution_binding.credential is required")

    return {
        "mode": "agent_cli",
        "agent_backend": str(binding.agent_backend).strip() or "claude",
        "model": model,
        "api_key": credential,
        "base_url": str(binding.base_url).strip(),
    }


def _resolve_item_name(input_payload: dict[str, object]) -> str:
    item_name = str(input_payload.get("item_name", "")).strip()
    if not item_name:
        raise ValueError("code.generate requires item_name")
    return item_name


def _resolve_project_root(input_payload: dict[str, object]) -> Path:
    root_text = str(input_payload.get("server_workspace_root", "")).strip()
    if not root_text:
        raise ValueError("code.generate requires server_workspace_root")
    project_root = Path(root_text)
    if not project_root.exists():
        raise ValueError(f"server workspace root does not exist: {project_root}")
    return project_root


def _resolve_implementation_notes(input_payload: dict[str, object]) -> str:
    analysis = str(input_payload.get("analysis", "")).strip()
    if analysis:
        return analysis
    implementation_notes = str(input_payload.get("implementation_notes", "")).strip()
    if implementation_notes:
        return implementation_notes
    raise ValueError("code.generate requires analysis or implementation_notes")


def _build_summary(full_text: str, item_name: str) -> str:
    for raw_line in full_text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.lower().startswith("summary:"):
            summary = line.split(":", 1)[1].strip()
            if summary:
                return summary[:120]
        if line.startswith("摘要："):
            summary = line[3:].strip()
            if summary:
                return summary[:120]
    return f"已写入 {item_name} 的服务器 custom_code 代码"


async def execute_code_generate_step(
    request: StepExecutionRequest,
    *,
    prompt_builder: Callable[[CustomCodegenRequest], str] = build_custom_code_prompt,
    code_agent_runner: CodeAgentRunner = run_agent_task_with_llm_config,
) -> dict[str, object]:
    item_name = _resolve_item_name(request.input_payload)
    description = str(request.input_payload.get("description", "")).strip()
    if not description:
        raise ValueError("code.generate requires description")

    project_root = _resolve_project_root(request.input_payload)
    implementation_notes = _resolve_implementation_notes(request.input_payload)
    prompt = prompt_builder(
        CustomCodegenRequest(
            description=description,
            implementation_notes=implementation_notes,
            name=item_name,
            project_root=project_root,
            skip_build=True,
        )
    )
    llm_cfg = build_code_llm_config(request.execution_binding)
    full_text = await code_agent_runner(prompt, project_root, llm_cfg)
    return {
        "text": _build_summary(full_text, item_name),
        "analysis": full_text,
        "item_name": item_name,
        "server_workspace_root": str(project_root),
    }
