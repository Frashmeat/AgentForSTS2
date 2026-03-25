from __future__ import annotations

import asyncio

from app.modules.workflow.application.step import WorkflowStep


class LinearExecutionPolicy:
    async def order(self, steps: list[WorkflowStep]) -> list[WorkflowStep]:
        return steps


class DagExecutionPolicy:
    async def order(self, steps: list[WorkflowStep]) -> list[WorkflowStep]:
        id_map = {step.name: step for step in steps}
        visited: set[str] = set()
        ordered: list[WorkflowStep] = []

        def visit(name: str):
            if name in visited or name not in id_map:
                return
            visited.add(name)
            for dep in id_map[name].depends_on:
                visit(dep)
            ordered.append(id_map[name])

        for step in steps:
            visit(step.name)
        return ordered


class LimitedParallelPolicy:
    def __init__(self, max_concurrency: int = 1) -> None:
        self.max_concurrency = max(1, max_concurrency)
        self._dag = DagExecutionPolicy()

    async def order(self, steps: list[WorkflowStep]) -> list[WorkflowStep]:
        return await self._dag.order(steps)

    def semaphore(self) -> asyncio.Semaphore:
        return asyncio.Semaphore(self.max_concurrency)
