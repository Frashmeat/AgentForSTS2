from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase


@dataclass(slots=True)
class StepExecutionRequest(ModelBase):
    workflow_version: str
    step_protocol_version: str
    step_type: str
    step_id: str
    job_id: int
    job_item_id: int
    result_schema_version: str
    input_payload: dict[str, object] = field(default_factory=dict)


@dataclass(slots=True)
class StepExecutionResult(ModelBase):
    step_id: str
    status: str
    output_payload: dict[str, object] = field(default_factory=dict)
    error_summary: str = ""
