from __future__ import annotations

from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field

from app.modules.workflow.application.context import WorkflowContext

StepHandler = Callable[[WorkflowContext], Awaitable[object]]


@dataclass
class WorkflowStep:
    name: str
    handler: StepHandler
    depends_on: list[str] = field(default_factory=list)
