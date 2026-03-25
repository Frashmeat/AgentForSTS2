from __future__ import annotations

from app.modules.workflow.application.context import WorkflowContext
from app.modules.workflow.application.engine import WorkflowEngine
from app.modules.workflow.application.policies import LimitedParallelPolicy
from app.modules.workflow.application.step import WorkflowStep


class BatchAssetWorkflow:
    def __init__(self, engine: WorkflowEngine | None = None, max_concurrency: int = 1) -> None:
        self.engine = engine or WorkflowEngine(policy=LimitedParallelPolicy(max_concurrency=max_concurrency))

    async def run(self, steps: list[WorkflowStep], context: WorkflowContext | None = None) -> WorkflowContext:
        workflow_context = context or WorkflowContext()
        return await self.engine.execute(steps, workflow_context)
