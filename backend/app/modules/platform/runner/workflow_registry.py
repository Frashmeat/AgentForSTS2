from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(slots=True)
class PlatformWorkflowStep:
    step_type: str
    step_id: str
    input_payload: dict[str, object] = field(default_factory=dict)


class PlatformWorkflowRegistry:
    def __init__(
        self,
        mappings: dict[tuple[str, str], list[PlatformWorkflowStep]] | None = None,
    ) -> None:
        self._mappings = mappings or {}

    def register(self, job_type: str, item_type: str, steps: list[PlatformWorkflowStep]) -> None:
        self._mappings[(job_type, item_type)] = list(steps)

    def resolve(self, job_type: str, item_type: str) -> list[PlatformWorkflowStep]:
        key = (job_type, item_type)
        if key not in self._mappings:
            raise KeyError(f"workflow not found for {job_type}/{item_type}")
        return list(self._mappings[key])
