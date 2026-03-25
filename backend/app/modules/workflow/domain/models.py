from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class WorkflowStepState:
    name: str
    status: str = "pending"
    output: Any = None


@dataclass
class WorkflowRun:
    run_id: str
    states: list[WorkflowStepState] = field(default_factory=list)
