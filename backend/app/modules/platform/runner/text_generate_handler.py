from __future__ import annotations

import logging
from collections.abc import Awaitable, Callable
from pathlib import Path

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.errors import build_platform_error

from .upstream_error_classifier import UpstreamErrorClassification, classify_upstream_error

CompleteTextFn = Callable[[str, dict[str, object], Path | None], Awaitable[str]]
logger = logging.getLogger(__name__)
_RUNTIME_SURFACE_PAYLOAD_KEY = "__runtime_surface"


def _short_text(value: object, limit: int = 300) -> str:
    text = str(value or "").replace("\n", "\\n").strip()
    if len(text) <= limit:
        return text
    return f"{text[:limit]}..."


def _upstream_diagnostic(error: Exception, classification: UpstreamErrorClassification) -> dict[str, object]:
    diagnostic: dict[str, object] = {
        "raw_error": classification.raw_error,
        "upstream_category": classification.upstream_category,
    }
    for attr in ("initial_error", "final_error", "endpoint"):
        value = getattr(error, attr, None)
        if value:
            diagnostic[attr] = str(value)
    attempts = getattr(error, "upstream_attempts", None)
    if isinstance(attempts, list):
        diagnostic["upstream_attempts"] = attempts
    return diagnostic


class UpstreamTextGenerationError(RuntimeError):
    def __init__(
        self,
        classification: UpstreamErrorClassification,
        *,
        runtime_surface: str,
        request: StepExecutionRequest,
    ) -> None:
        super().__init__(classification.reason_message)
        self.classification = classification
        self.raw_error = classification.raw_error
        self.reason_code = classification.reason_code
        self.runtime_surface = runtime_surface
        self.request = request

    def to_error_payload(self) -> dict[str, object]:
        return build_platform_error(
            origin="upstream",
            runtime_surface=self.runtime_surface,
            component="text_generate",
            operation="complete_text",
            category=self.classification.upstream_category,
            reason_code=self.reason_code,
            message=str(self),
            developer_message=self.raw_error,
            retryable=self.classification.retryable,
            step_id=self.request.step_id,
            step_type=self.request.step_type,
            job_id=self.request.job_id,
            job_item_id=self.request.job_item_id,
            api_protocol=self.request.execution_binding.api_protocol,
            model=self.request.execution_binding.model,
            http_status=self.classification.http_status,
            provider_error_code=self.classification.provider_error_code,
            log_hint={
                "primary": f"{self.runtime_surface}_log",
                "secondary": "web_backend_log",
            },
            diagnostic=_upstream_diagnostic(self.__cause__ or self, self.classification),
        ).to_payload()


class UpstreamTextGenerationBlockedError(UpstreamTextGenerationError):
    pass


def build_text_llm_config(binding: StepExecutionBinding) -> dict[str, object]:
    if not str(binding.model).strip():
        raise ValueError("execution_binding.model is required")
    if not str(binding.credential).strip():
        raise ValueError("execution_binding.credential is required")

    runner_type = str(binding.runner_type).strip() or "claude_cli"
    if runner_type not in {"codex_cli", "claude_cli", "api"}:
        raise ValueError("execution_binding.runner_type must be codex_cli, claude_cli or api")
    api_protocol = str(binding.api_protocol).strip()
    provider = _api_protocol_to_litellm_provider(api_protocol)
    agent_backend = "codex" if runner_type == "codex_cli" else "claude"
    return {
        "mode": "agent_cli" if runner_type in {"codex_cli", "claude_cli"} else "api",
        "agent_backend": agent_backend,
        "provider": provider,
        "model": str(binding.model).strip(),
        "api_key": str(binding.credential).strip(),
        "base_url": str(binding.api_base_url).strip(),
    }


def _api_protocol_to_litellm_provider(api_protocol: str) -> str:
    if api_protocol == "openai_compatible":
        return "openai"
    if api_protocol == "anthropic_compatible":
        return "anthropic"
    if api_protocol:
        raise ValueError("execution_binding.api_protocol must be openai_compatible or anthropic_compatible")
    return ""


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
    runtime_surface = _resolve_runtime_surface(request)
    logger.info(
        "platform text generation start job_id=%s job_item_id=%s step_id=%s api_protocol=%s model=%s "
        "base_url_configured=%s prompt_len=%d",
        request.job_id,
        request.job_item_id,
        request.step_id,
        request.execution_binding.api_protocol,
        request.execution_binding.model,
        bool(str(request.execution_binding.api_base_url).strip()),
        len(prompt),
    )
    try:
        output = await complete_text_fn(prompt, llm_cfg, None)
    except Exception as error:
        classification = classify_upstream_error(error)
        if classification.reason_code != "upstream_unclassified_error":
            logger.warning(
                "platform text generation upstream failed job_id=%s job_item_id=%s step_id=%s api_protocol=%s model=%s "
                "reason_code=%s upstream_category=%s retryable=%s http_status=%s provider_error_code=%s raw_error=%s",
                request.job_id,
                request.job_item_id,
                request.step_id,
                request.execution_binding.api_protocol,
                request.execution_binding.model,
                classification.reason_code,
                classification.upstream_category,
                classification.retryable,
                classification.http_status,
                classification.provider_error_code,
                _short_text(error),
            )
            raise UpstreamTextGenerationBlockedError(
                classification,
                runtime_surface=runtime_surface,
                request=request,
            ) from error
        logger.exception(
            "platform text generation failed job_id=%s job_item_id=%s step_id=%s api_protocol=%s model=%s error=%s",
            request.job_id,
            request.job_item_id,
            request.step_id,
            request.execution_binding.api_protocol,
            request.execution_binding.model,
            _short_text(error),
        )
        raise
    logger.info(
        "platform text generation succeeded job_id=%s job_item_id=%s step_id=%s api_protocol=%s model=%s output_len=%d",
        request.job_id,
        request.job_item_id,
        request.step_id,
        request.execution_binding.api_protocol,
        request.execution_binding.model,
        len(output),
    )
    return {
        "text": output,
        "api_protocol": request.execution_binding.api_protocol,
        "model": request.execution_binding.model,
    }


def _resolve_runtime_surface(request: StepExecutionRequest) -> str:
    value = str(request.input_payload.get(_RUNTIME_SURFACE_PAYLOAD_KEY) or "").strip()
    if value in {"web", "web_workstation", "local_workstation"}:
        return value
    return "web"
