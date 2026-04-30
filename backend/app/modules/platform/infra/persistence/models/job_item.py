from __future__ import annotations

from typing import TYPE_CHECKING

from sqlalchemy import DateTime, ForeignKey, Index, Integer, String, Text, UniqueConstraint, text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.modules.platform.domain.models.enums import JobItemStatus
from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, json_type, str_enum_type

if TYPE_CHECKING:
    from .job import JobRecord


class JobItemRecord(TimestampMixin, Base):
    __tablename__ = "job_items"
    __table_args__ = (
        UniqueConstraint("job_id", "item_index", name="uq_job_items_job_id_item_index"),
        UniqueConstraint("job_id", "id", name="uq_job_items_job_id_id"),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    job_id: Mapped[int] = mapped_column(ForeignKey("jobs.id"), nullable=False)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    item_index: Mapped[int] = mapped_column(Integer, nullable=False)
    item_type: Mapped[str] = mapped_column(String(64), nullable=False)
    status: Mapped[JobItemStatus] = mapped_column(
        str_enum_type(JobItemStatus, "job_item_status"),
        nullable=False,
        default=JobItemStatus.PENDING,
    )
    input_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    input_payload: Mapped[dict[str, object]] = mapped_column(
        json_type(),
        nullable=False,
        server_default=text("'{}'"),
        default=dict,
    )
    result_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    error_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    attempt_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    started_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    finished_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    job: Mapped[JobRecord] = relationship("JobRecord", back_populates="items")


Index("ix_job_items_job_id_item_index", JobItemRecord.job_id, JobItemRecord.item_index)
