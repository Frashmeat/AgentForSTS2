from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest, StepExecutionResult


StepHandler = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]


class ExecutionAdapter:
    def __init__(
        self,
        *,
        image_handler: StepHandler | None,
        code_handler: StepHandler | None,
        text_handler: StepHandler | None,
        batch_custom_code_handler: StepHandler | None,
        log_handler: StepHandler | None,
        build_handler: StepHandler | None,
        approval_handler: StepHandler | None,
    ) -> None:
        self._handlers = {
            "image.generate": image_handler,
            "code.generate": code_handler,
            "text.generate": text_handler,
            "batch.custom_code.plan": batch_custom_code_handler,
            "log.analyze": log_handler,
            "build.project": build_handler,
            "approval.request": approval_handler,
        }

    async def execute(self, request: StepExecutionRequest) -> StepExecutionResult:
        handler = self._handlers.get(request.step_type)
        if handler is None:
            return StepExecutionResult(
                step_id=request.step_id,
                status="failed_system",
                error_summary=f"unsupported step_type: {request.step_type}",
            )
        try:
            payload = await handler(request)
            return StepExecutionResult(
                step_id=request.step_id,
                status="succeeded",
                output_payload=dict(payload),
            )
        except Exception as exc:
            return StepExecutionResult(
                step_id=request.step_id,
                status="failed_system",
                error_summary=str(exc),
            )
