from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class WorkflowContext:
    values: dict[str, Any] = field(default_factory=dict)
    interrupted: bool = False

    def get(self, key: str, default=None):
        return self.values.get(key, default)

    def set(self, key: str, value: Any) -> None:
        self.values[key] = value

    def interrupt(self) -> None:
        self.interrupted = True
