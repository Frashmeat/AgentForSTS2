from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest, StepExecutionResult


ExecuteHandler = Callable[[StepExecutionRequest], Awaitable[StepExecutionResult]]


class StepDispatcher:
    def __init__(self, execute_handler: ExecuteHandler) -> None:
        self.execute_handler = execute_handler

    async def dispatch(self, request: StepExecutionRequest) -> StepExecutionResult:
        return await self.execute_handler(request)
