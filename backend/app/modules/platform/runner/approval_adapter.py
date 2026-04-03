from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest


ApprovalHandler = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]


class ApprovalAdapter:
    def __init__(self, handler: ApprovalHandler) -> None:
        self.handler = handler

    async def execute(self, request: StepExecutionRequest) -> dict[str, object]:
        return await self.handler(request)
