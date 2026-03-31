from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase


@dataclass(slots=True)
class PlatformEventCursor(ModelBase):
    after_id: int | None = None
    limit: int = 50


@dataclass(slots=True)
class JobEventView(ModelBase):
    event_id: int
    event_type: str
    job_id: int
    occurred_at: str
    payload: dict[str, object] = field(default_factory=dict)
    job_item_id: int | None = None
    ai_execution_id: int | None = None

    def as_user_payload(self) -> dict[str, object]:
        return self.model_dump(exclude={"ai_execution_id"}, exclude_none=True)
