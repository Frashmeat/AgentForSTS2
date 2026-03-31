from __future__ import annotations

from sqlalchemy import DateTime, ForeignKeyConstraint, Index, String, text
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, json_type


class JobEventRecord(TimestampMixin, Base):
    __tablename__ = "job_events"
    __table_args__ = (
        ForeignKeyConstraint(["job_id"], ["jobs.id"], name="fk_job_events_job_id_jobs"),
        ForeignKeyConstraint(
            ["job_item_id", "job_id"],
            ["job_items.id", "job_items.job_id"],
            name="fk_job_events_job_item_chain",
        ),
        ForeignKeyConstraint(
            ["ai_execution_id"],
            ["ai_executions.id"],
            name="fk_job_events_ai_execution_id_ai_executions",
        ),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    job_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    job_item_id: Mapped[int | None] = mapped_column(bigint_type(), nullable=True)
    ai_execution_id: Mapped[int | None] = mapped_column(bigint_type(), nullable=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    event_type: Mapped[str] = mapped_column(String(128), nullable=False)
    event_payload: Mapped[dict[str, object]] = mapped_column(
        json_type(),
        nullable=False,
        server_default=text("'{}'"),
        default=dict,
    )
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_job_events_job_id_created_at", JobEventRecord.job_id, JobEventRecord.created_at)
