from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field


@dataclass(slots=True)
class PlatformWorkflowStep:
    step_type: str
    step_id: str
    input_payload: dict[str, object] = field(default_factory=dict)


WorkflowResolver = Callable[[dict[str, object]], list[PlatformWorkflowStep]]


class PlatformWorkflowRegistry:
    def __init__(
        self,
        mappings: dict[tuple[str, str], list[PlatformWorkflowStep] | WorkflowResolver] | None = None,
    ) -> None:
        self._mappings = mappings or {}

    def register(
        self,
        job_type: str,
        item_type: str,
        steps: list[PlatformWorkflowStep] | WorkflowResolver,
    ) -> None:
        self._mappings[(job_type, item_type)] = steps

    def resolve(
        self,
        job_type: str,
        item_type: str,
        input_payload: dict[str, object] | None = None,
    ) -> list[PlatformWorkflowStep]:
        key = (job_type, item_type)
        if key not in self._mappings:
            raise KeyError(f"workflow not found for {job_type}/{item_type}")
        mapping = self._mappings[key]
        if callable(mapping):
            return list(mapping(dict(input_payload or {})))
        return list(mapping)
