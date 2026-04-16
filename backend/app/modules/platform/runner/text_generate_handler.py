from __future__ import annotations

from collections.abc import Awaitable, Callable
from pathlib import Path

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest


CompleteTextFn = Callable[[str, dict[str, object], Path | None], Awaitable[str]]


def build_text_llm_config(binding: StepExecutionBinding) -> dict[str, object]:
    if not str(binding.model).strip():
        raise ValueError("execution_binding.model is required")
    if not str(binding.credential).strip():
        raise ValueError("execution_binding.credential is required")

    return {
        "mode": "claude_api",
        "agent_backend": str(binding.agent_backend).strip() or "claude",
        "model": str(binding.model).strip(),
        "api_key": str(binding.credential).strip(),
        "base_url": str(binding.base_url).strip(),
    }


async def execute_text_generate_step(
    request: StepExecutionRequest,
    *,
    complete_text_fn: CompleteTextFn | None = None,
) -> dict[str, object]:
    prompt = str(request.input_payload.get("prompt", "")).strip()
    if not prompt:
        raise ValueError("input_payload.prompt is required")
    if complete_text_fn is None:
        from llm.text_runner import complete_text as default_complete_text

        complete_text_fn = default_complete_text

    llm_cfg = build_text_llm_config(request.execution_binding)
    output = await complete_text_fn(prompt, llm_cfg, None)
    return {
        "text": output,
        "provider": request.execution_binding.provider,
        "model": request.execution_binding.model,
    }
