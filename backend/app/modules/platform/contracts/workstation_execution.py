from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase
from .runner_contracts import StepExecutionBinding


@dataclass(slots=True)
class WorkstationCallbackConfig(ModelBase):
    enabled: bool = False
    url: str = ""
    token_ref: str = ""
    auth: str = "none"
    signature_version: str = ""


@dataclass(slots=True)
class WorkstationArtifactPayload(ModelBase):
    artifact_type: str
    storage_provider: str
    object_key: str
    file_name: str = ""
    mime_type: str = ""
    size_bytes: int = 0
    result_summary: str = ""


@dataclass(slots=True)
class WorkstationExecutionDispatchRequest(ModelBase):
    execution_id: int
    job_id: int
    job_item_id: int
    job_type: str
    item_type: str
    workflow_version: str
    step_protocol_version: str
    result_schema_version: str
    input_payload: dict[str, object] = field(default_factory=dict)
    execution_binding: StepExecutionBinding = field(default_factory=StepExecutionBinding)
    callback: WorkstationCallbackConfig = field(default_factory=WorkstationCallbackConfig)


@dataclass(slots=True)
class WorkstationExecutionDispatchAccepted(ModelBase):
    workstation_execution_id: str
    status: str = "accepted"
    poll_url: str = ""


@dataclass(slots=True)
class WorkstationExecutionPollResult(ModelBase):
    workstation_execution_id: str
    status: str
    step_id: str
    output_payload: dict[str, object] = field(default_factory=dict)
    error_summary: str = ""
    error_payload: dict[str, object] = field(default_factory=dict)
