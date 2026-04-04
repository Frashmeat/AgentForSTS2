from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase


@dataclass(slots=True)
class JobListItem(ModelBase):
    id: int
    job_type: str
    status: str
    input_summary: str = ""
    result_summary: str = ""
    total_item_count: int = 0
    succeeded_item_count: int = 0
    failed_item_count: int = 0
    original_deducted: int = 0
    refunded_amount: int = 0
    net_consumed: int = 0
    refund_reason_summary: str = ""


@dataclass(slots=True)
class JobItemListItem(ModelBase):
    id: int
    item_index: int
    item_type: str
    status: str
    result_summary: str = ""
    error_summary: str = ""


@dataclass(slots=True)
class ArtifactSummary(ModelBase):
    id: int
    artifact_type: str
    file_name: str | None = None
    result_summary: str = ""


@dataclass(slots=True)
class JobDetailView(ModelBase):
    id: int
    job_type: str
    status: str
    input_summary: str = ""
    result_summary: str = ""
    error_summary: str = ""
    original_deducted: int = 0
    refunded_amount: int = 0
    net_consumed: int = 0
    refund_reason_summary: str = ""
    items: list[JobItemListItem] = field(default_factory=list)
    artifacts: list[ArtifactSummary] = field(default_factory=list)


@dataclass(slots=True)
class UserQuotaView(ModelBase):
    daily_limit: int = 0
    daily_used: int = 0
    weekly_limit: int = 0
    weekly_used: int = 0
    refunded: int = 0
    next_reset_at: str | None = None
