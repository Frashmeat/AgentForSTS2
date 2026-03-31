from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase


@dataclass(slots=True)
class CreateJobItemInput(ModelBase):
    item_type: str
    input_summary: str = ""
    input_payload: dict[str, object] = field(default_factory=dict)


@dataclass(slots=True)
class CreateJobCommand(ModelBase):
    job_type: str
    workflow_version: str
    input_summary: str = ""
    created_from: str = "platform_api"
    items: list[CreateJobItemInput] = field(default_factory=list)


@dataclass(slots=True)
class StartJobCommand(ModelBase):
    job_id: int
    triggered_by: str = "user"


@dataclass(slots=True)
class CancelJobCommand(ModelBase):
    job_id: int
    reason: str = ""
