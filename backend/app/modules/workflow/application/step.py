from __future__ import annotations

from dataclasses import dataclass, field
from typing import Awaitable, Callable

from app.modules.workflow.application.context import WorkflowContext


StepHandler = Callable[[WorkflowContext], Awaitable[object]]


@dataclass
class WorkflowStep:
    name: str
    handler: StepHandler
    depends_on: list[str] = field(default_factory=list)
