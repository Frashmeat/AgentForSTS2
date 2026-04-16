from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase


@dataclass(slots=True)
class StepExecutionBinding(ModelBase):
    agent_backend: str = ""
    provider: str = ""
    model: str = ""
    credential_ref: str = ""
    auth_type: str = ""
    credential: str = ""
    secret: str = ""
    base_url: str = ""
    retry_attempt: int = 0
    switched_credential: bool = False


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
    execution_binding: StepExecutionBinding = field(default_factory=StepExecutionBinding)


@dataclass(slots=True)
class StepExecutionResult(ModelBase):
    step_id: str
    status: str
    output_payload: dict[str, object] = field(default_factory=dict)
    error_summary: str = ""
