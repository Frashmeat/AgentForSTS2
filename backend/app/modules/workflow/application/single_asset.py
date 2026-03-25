from __future__ import annotations

from app.modules.workflow.application.context import WorkflowContext
from app.modules.workflow.application.engine import WorkflowEngine
from app.modules.workflow.application.step import WorkflowStep


class SingleAssetWorkflow:
    def __init__(self, engine: WorkflowEngine | None = None) -> None:
        self.engine = engine or WorkflowEngine()

    async def run(self, steps: list[WorkflowStep], context: WorkflowContext | None = None) -> WorkflowContext:
        workflow_context = context or WorkflowContext()
        return await self.engine.execute(steps, workflow_context)
