from __future__ import annotations

from typing import TYPE_CHECKING

from sqlalchemy import DateTime, Index, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.modules.platform.domain.models.enums import JobStatus
from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, str_enum_type

if TYPE_CHECKING:
    from .job_item import JobItemRecord


class JobRecord(TimestampMixin, Base):
    __tablename__ = "jobs"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    job_type: Mapped[str] = mapped_column(String(64), nullable=False)
    status: Mapped[JobStatus] = mapped_column(
        str_enum_type(JobStatus, "job_status"),
        nullable=False,
        default=JobStatus.DRAFT,
    )
    workflow_version: Mapped[str] = mapped_column(String(32), nullable=False)
    input_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    selected_execution_profile_id: Mapped[int | None] = mapped_column(bigint_type(), nullable=True)
    selected_agent_backend: Mapped[str] = mapped_column(String(32), nullable=False, default="")
    selected_model: Mapped[str] = mapped_column(String(128), nullable=False, default="")
    result_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    error_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    total_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    pending_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    running_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    succeeded_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    failed_business_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    failed_system_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    quota_skipped_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    cancelled_before_start_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    cancelled_after_start_item_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    started_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    finished_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    cancel_requested_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    items: Mapped[list["JobItemRecord"]] = relationship(
        "JobItemRecord",
        back_populates="job",
        cascade="all, delete-orphan",
        order_by="JobItemRecord.item_index",
    )


Index("ix_jobs_user_id_created_at_desc", JobRecord.user_id, JobRecord.created_at.desc())
Index("ix_jobs_status_created_at_desc", JobRecord.status, JobRecord.created_at.desc())
