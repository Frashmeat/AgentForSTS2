from __future__ import annotations

import logging
from collections.abc import Awaitable, Callable
from pathlib import Path

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from .upstream_error_classifier import UpstreamErrorClassification, classify_upstream_error


CompleteTextFn = Callable[[str, dict[str, object], Path | None], Awaitable[str]]
logger = logging.getLogger(__name__)


def _short_text(value: object, limit: int = 300) -> str:
    text = str(value or "").replace("\n", "\\n").strip()
    if len(text) <= limit:
        return text
    return f"{text[:limit]}..."


class UpstreamTextGenerationError(RuntimeError):
    def __init__(self, classification: UpstreamErrorClassification) -> None:
        super().__init__(classification.reason_message)
        self.classification = classification
        self.raw_error = classification.raw_error
        self.reason_code = classification.reason_code

    def to_error_payload(self) -> dict[str, object]:
        return {
            "reason_code": self.reason_code,
            "reason_message": str(self),
            "upstream_category": self.classification.upstream_category,
            "retryable": self.classification.retryable,
            "http_status": self.classification.http_status,
            "provider_error_code": self.classification.provider_error_code,
            "raw_error": self.raw_error,
        }


class UpstreamTextGenerationBlockedError(UpstreamTextGenerationError):
    pass


def build_text_llm_config(binding: StepExecutionBinding) -> dict[str, object]:
    if not str(binding.model).strip():
        raise ValueError("execution_binding.model is required")
    if not str(binding.credential).strip():
        raise ValueError("execution_binding.credential is required")

    agent_backend = str(binding.agent_backend).strip() or "claude"
    return {
        "mode": "agent_cli" if agent_backend == "codex" else "claude_api",
        "agent_backend": agent_backend,
        "provider": str(binding.provider).strip(),
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
        classification = classify_upstream_error(error)
        if classification.reason_code != "upstream_unclassified_error":
            logger.warning(
                "platform text generation upstream failed job_id=%s job_item_id=%s step_id=%s provider=%s model=%s "
                "reason_code=%s upstream_category=%s retryable=%s http_status=%s provider_error_code=%s raw_error=%s",
                request.job_id,
                request.job_item_id,
                request.step_id,
                request.execution_binding.provider,
                request.execution_binding.model,
                classification.reason_code,
                classification.upstream_category,
                classification.retryable,
                classification.http_status,
                classification.provider_error_code,
                _short_text(error),
            )
            raise UpstreamTextGenerationBlockedError(classification) from error
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
