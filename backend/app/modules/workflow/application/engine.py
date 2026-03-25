from __future__ import annotations

from collections.abc import Awaitable, Callable

from app.shared.contracts.workflow_events import WorkflowEvent

from .context import WorkflowContext
from .policies import LinearExecutionPolicy
from .step import WorkflowStep


EventPublisher = Callable[[WorkflowEvent], Awaitable[None]]


class WorkflowEngine:
    def __init__(self, policy=None, publisher: EventPublisher | None = None) -> None:
        self.policy = policy or LinearExecutionPolicy()
        self.publisher = publisher

    async def execute(self, steps: list[WorkflowStep], context: WorkflowContext) -> WorkflowContext:
        ordered_steps = await self.policy.order(steps)
        semaphore_factory = getattr(self.policy, "semaphore", None)
        semaphore = semaphore_factory() if callable(semaphore_factory) else None

        for step in ordered_steps:
            if context.interrupted:
                break
            if self.publisher is not None:
                await self.publisher(WorkflowEvent(stage=step.name, payload={"status": "started"}))
            try:
                if semaphore is not None:
                    async with semaphore:
                        result = await step.handler(context)
                else:
                    result = await step.handler(context)
                context.set(step.name, result)
                if self.publisher is not None:
                    await self.publisher(WorkflowEvent(stage=step.name, payload={"status": "completed", "result": result}))
            except Exception as exc:
                if self.publisher is not None:
                    await self.publisher(WorkflowEvent(stage="error", payload={"step": step.name, "message": str(exc)}))
                raise
        return context
