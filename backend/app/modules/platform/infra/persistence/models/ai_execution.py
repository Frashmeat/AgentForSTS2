from __future__ import annotations

from sqlalchemy import DateTime, ForeignKeyConstraint, Index, String, Text, text
from sqlalchemy.orm import Mapped, mapped_column

from app.modules.platform.domain.models.enums import AIExecutionStatus
from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, json_type, str_enum_type


class AIExecutionRecord(TimestampMixin, Base):
    __tablename__ = "ai_executions"
    __table_args__ = (
        ForeignKeyConstraint(
            ["job_id"],
            ["jobs.id"],
            name="fk_ai_executions_job_id_jobs",
        ),
        ForeignKeyConstraint(
            ["job_id", "job_item_id"],
            ["job_items.job_id", "job_items.id"],
            name="fk_ai_executions_job_item_chain",
        ),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    job_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    job_item_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    status: Mapped[AIExecutionStatus] = mapped_column(
        str_enum_type(AIExecutionStatus, "ai_execution_status"),
        nullable=False,
        default=AIExecutionStatus.CREATED,
    )
    provider: Mapped[str] = mapped_column(String(64), nullable=False)
    model: Mapped[str] = mapped_column(String(128), nullable=False)
    request_idempotency_key: Mapped[str | None] = mapped_column(String(128), nullable=True)
    workflow_version: Mapped[str] = mapped_column(String(32), nullable=False)
    step_protocol_version: Mapped[str] = mapped_column(String(32), nullable=False)
    result_schema_version: Mapped[str] = mapped_column(String(32), nullable=False)
    step_type: Mapped[str] = mapped_column(String(64), nullable=False)
    step_id: Mapped[str] = mapped_column(String(128), nullable=False)
    input_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    input_payload: Mapped[dict[str, object]] = mapped_column(
        json_type(),
        nullable=False,
        server_default=text("'{}'"),
        default=dict,
    )
    result_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    result_payload: Mapped[dict[str, object]] = mapped_column(
        json_type(),
        nullable=False,
        server_default=text("'{}'"),
        default=dict,
    )
    error_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    error_payload: Mapped[dict[str, object]] = mapped_column(
        json_type(),
        nullable=False,
        server_default=text("'{}'"),
        default=dict,
    )
    started_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    finished_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_ai_executions_job_item_id_created_at", AIExecutionRecord.job_item_id, AIExecutionRecord.created_at)
Index(
    "uq_ai_executions_user_item_idempotency_key",
    AIExecutionRecord.user_id,
    AIExecutionRecord.job_item_id,
    AIExecutionRecord.request_idempotency_key,
    unique=True,
    postgresql_where=AIExecutionRecord.request_idempotency_key.is_not(None),
)
