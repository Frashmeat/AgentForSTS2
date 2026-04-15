from __future__ import annotations

from dataclasses import dataclass

from ._model import ModelBase


@dataclass(slots=True)
class AdminExecutionListItem(ModelBase):
    id: int
    job_id: int
    job_item_id: int
    status: str
    provider: str
    model: str
    credential_ref: str = ""
    retry_attempt: int = 0
    switched_credential: bool = False


@dataclass(slots=True)
class AdminExecutionDetailView(ModelBase):
    id: int
    job_id: int
    job_item_id: int
    status: str
    provider: str
    model: str
    credential_ref: str = ""
    retry_attempt: int = 0
    switched_credential: bool = False
    request_idempotency_key: str | None = None
    input_summary: str = ""
    result_summary: str = ""
    error_summary: str = ""
    step_protocol_version: str | None = None
    result_schema_version: str | None = None


@dataclass(slots=True)
class RefundRecordView(ModelBase):
    ai_execution_id: int
    charge_status: str
    refund_reason: str = ""


@dataclass(slots=True)
class AdminServerCredentialListItem(ModelBase):
    id: int
    execution_profile_id: int
    provider: str
    auth_type: str
    label: str
    base_url: str
    priority: int
    enabled: bool
    health_status: str
    last_checked_at: str | None = None
    last_error_code: str = ""
    last_error_message: str = ""


@dataclass(slots=True)
class AdminServerCredentialHealthCheckView(ModelBase):
    credential_id: int
    health_status: str
    error_code: str = ""
    error_message: str = ""
    checked_at: str | None = None


@dataclass(slots=True)
class AdminExecutionProfileListItem(ModelBase):
    id: int
    code: str
    display_name: str
    agent_backend: str
    model: str
    enabled: bool
    recommended: bool
    sort_order: int
