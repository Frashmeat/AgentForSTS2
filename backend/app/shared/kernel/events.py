from dataclasses import dataclass, field
from typing import Any


@dataclass(slots=True)
class WorkflowEvent:
    stage: str
    payload: dict[str, Any] = field(default_factory=dict)
