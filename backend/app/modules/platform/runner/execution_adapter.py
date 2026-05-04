from __future__ import annotations

import logging
from collections.abc import Awaitable, Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest, StepExecutionResult
from app.modules.platform.errors import error_payload_from_exception

StepHandler = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]
logger = logging.getLogger(__name__)
_RUNTIME_SURFACE_PAYLOAD_KEY = "__runtime_surface"


def _short_text(value: object, limit: int = 300) -> str:
    text = str(value or "").replace("\n", "\\n").strip()
    if len(text) <= limit:
        return text
    return f"{text[:limit]}..."


class ExecutionAdapter:
    def __init__(
        self,
        *,
        image_handler: StepHandler | None,
        code_handler: StepHandler | None,
        asset_handler: StepHandler | None,
        text_handler: StepHandler | None,
        batch_custom_code_handler: StepHandler | None,
        single_asset_plan_handler: StepHandler | None,
        log_handler: StepHandler | None,
        build_handler: StepHandler | None,
        approval_handler: StepHandler | None,
    ) -> None:
        self._handlers = {
            "image.generate": image_handler,
            "code.generate": code_handler,
            "asset.generate": asset_handler,
            "text.generate": text_handler,
            "batch.custom_code.plan": batch_custom_code_handler,
            "single.asset.plan": single_asset_plan_handler,
            "log.analyze": log_handler,
            "build.project": build_handler,
            "approval.request": approval_handler,
        }

    async def execute(self, request: StepExecutionRequest) -> StepExecutionResult:
        handler = self._handlers.get(request.step_type)
        if handler is None:
            logger.warning(
                "platform step unsupported job_id=%s job_item_id=%s step_type=%s step_id=%s",
                request.job_id,
                request.job_item_id,
                request.step_type,
                request.step_id,
            )
            return StepExecutionResult(
                step_id=request.step_id,
                status="failed_system",
                error_summary=f"unsupported step_type: {request.step_type}",
            )
        logger.info(
            "platform step start job_id=%s job_item_id=%s step_type=%s step_id=%s",
            request.job_id,
            request.job_item_id,
            request.step_type,
            request.step_id,
        )
        try:
            payload = await handler(request)
            logger.info(
                "platform step succeeded job_id=%s job_item_id=%s step_type=%s step_id=%s output_keys=%s",
                request.job_id,
                request.job_item_id,
                request.step_type,
                request.step_id,
                sorted(str(key) for key in payload),
            )
            return StepExecutionResult(
                step_id=request.step_id,
                status="succeeded",
                output_payload=dict(payload),
            )
        except Exception as exc:
            runtime_surface = _resolve_runtime_surface(request)
            error_payload = error_payload_from_exception(
                exc,
                runtime_surface=runtime_surface,
                component=request.step_type,
                operation=request.step_id,
                step_id=request.step_id,
                step_type=request.step_type,
                job_id=request.job_id,
                job_item_id=request.job_item_id,
            )
            reason_code = str(error_payload.get("reason_code", "")).strip()
            if reason_code:
                logger.warning(
                    "platform step failed job_id=%s job_item_id=%s step_type=%s step_id=%s reason_code=%s error=%s",
                    request.job_id,
                    request.job_item_id,
                    request.step_type,
                    request.step_id,
                    reason_code,
                    _short_text(exc),
                )
            else:
                logger.exception(
                    "platform step failed job_id=%s job_item_id=%s step_type=%s step_id=%s error=%s",
                    request.job_id,
                    request.job_item_id,
                    request.step_type,
                    request.step_id,
                    _short_text(exc),
                )
            return StepExecutionResult(
                step_id=request.step_id,
                status="failed_system",
                error_summary=str(error_payload.get("message") or exc),
                error_payload=error_payload,
            )


def _resolve_runtime_surface(request: StepExecutionRequest) -> str:
    value = str(request.input_payload.get(_RUNTIME_SURFACE_PAYLOAD_KEY) or "").strip()
    if value in {"web", "web_workstation", "local_workstation"}:
        return value
    return "web"
