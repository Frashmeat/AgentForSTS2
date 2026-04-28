from __future__ import annotations

import logging
from collections.abc import Awaitable, Callable
from pathlib import Path

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest


CompleteTextFn = Callable[[str, dict[str, object], Path | None], Awaitable[str]]
logger = logging.getLogger(__name__)


def _short_text(value: object, limit: int = 300) -> str:
    text = str(value or "").replace("\n", "\\n").strip()
    if len(text) <= limit:
        return text
    return f"{text[:limit]}..."


class UpstreamTextGenerationBlockedError(RuntimeError):
    reason_code = "upstream_request_blocked"

    def __init__(self, raw_error: str) -> None:
        super().__init__(
            "上游模型拒绝了这次请求。该描述可能触发内容安全策略；请改写描述或切换可用的执行配置后重试。"
        )
        self.raw_error = raw_error

    def to_error_payload(self) -> dict[str, object]:
        return {
            "reason_code": self.reason_code,
            "reason_message": str(self),
            "raw_error": self.raw_error,
        }


def _is_upstream_request_blocked(error: Exception) -> bool:
    text = str(error).lower()
    return any(
        marker in text
        for marker in (
            "request was blocked",
            "content policy",
            "safety policy",
            "content_filter",
            "content filter",
        )
    )


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
    logger.info(
        "platform text generation start job_id=%s job_item_id=%s step_id=%s provider=%s model=%s "
        "base_url_configured=%s prompt_len=%d",
        request.job_id,
        request.job_item_id,
        request.step_id,
        request.execution_binding.provider,
        request.execution_binding.model,
        bool(str(request.execution_binding.base_url).strip()),
        len(prompt),
    )
    try:
        output = await complete_text_fn(prompt, llm_cfg, None)
    except Exception as error:
        if _is_upstream_request_blocked(error):
            logger.warning(
                "platform text generation blocked job_id=%s job_item_id=%s step_id=%s provider=%s model=%s "
                "reason_code=%s raw_error=%s",
                request.job_id,
                request.job_item_id,
                request.step_id,
                request.execution_binding.provider,
                request.execution_binding.model,
                UpstreamTextGenerationBlockedError.reason_code,
                _short_text(error),
            )
            raise UpstreamTextGenerationBlockedError(str(error)) from error
        logger.exception(
            "platform text generation failed job_id=%s job_item_id=%s step_id=%s provider=%s model=%s error=%s",
            request.job_id,
            request.job_item_id,
            request.step_id,
            request.execution_binding.provider,
            request.execution_binding.model,
            _short_text(error),
        )
        raise
    logger.info(
        "platform text generation succeeded job_id=%s job_item_id=%s step_id=%s provider=%s model=%s output_len=%d",
        request.job_id,
        request.job_item_id,
        request.step_id,
        request.execution_binding.provider,
        request.execution_binding.model,
        len(output),
    )
    return {
        "text": output,
        "provider": request.execution_binding.provider,
        "model": request.execution_binding.model,
    }
