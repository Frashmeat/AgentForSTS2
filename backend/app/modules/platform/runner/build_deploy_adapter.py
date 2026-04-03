from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest


BuildHandler = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]


class BuildDeployAdapter:
    def __init__(self, handler: BuildHandler) -> None:
        self.handler = handler

    async def execute(self, request: StepExecutionRequest) -> dict[str, object]:
        return await self.handler(request)
