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


@dataclass(slots=True)
class AdminExecutionDetailView(ModelBase):
    id: int
    job_id: int
    job_item_id: int
    status: str
    provider: str
    model: str
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
