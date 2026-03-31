from __future__ import annotations

from enum import StrEnum


class JobStatus(StrEnum):
    DRAFT = "draft"
    QUEUED = "queued"
    RUNNING = "running"
    PARTIAL_SUCCEEDED = "partial_succeeded"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    QUOTA_EXHAUSTED = "quota_exhausted"
    CANCELLING = "cancelling"
    CANCELLED = "cancelled"


class JobItemStatus(StrEnum):
    PENDING = "pending"
    READY = "ready"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED_BUSINESS = "failed_business"
    FAILED_SYSTEM = "failed_system"
    QUOTA_SKIPPED = "quota_skipped"
    CANCELLED_BEFORE_START = "cancelled_before_start"
    CANCELLED_AFTER_START = "cancelled_after_start"


class AIExecutionStatus(StrEnum):
    CREATED = "created"
    DISPATCHING = "dispatching"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED_BUSINESS = "failed_business"
    FAILED_SYSTEM = "failed_system"
    COMPLETED_WITH_REFUND = "completed_with_refund"
