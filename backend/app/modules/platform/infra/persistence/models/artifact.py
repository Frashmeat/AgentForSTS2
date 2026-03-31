from __future__ import annotations

from sqlalchemy import DateTime, ForeignKeyConstraint, Index, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type


class ArtifactRecord(TimestampMixin, Base):
    __tablename__ = "artifacts"
    __table_args__ = (
        ForeignKeyConstraint(["job_id"], ["jobs.id"], name="fk_artifacts_job_id_jobs"),
        ForeignKeyConstraint(
            ["job_item_id", "job_id"],
            ["job_items.id", "job_items.job_id"],
            name="fk_artifacts_job_item_chain",
        ),
        ForeignKeyConstraint(
            ["ai_execution_id"],
            ["ai_executions.id"],
            name="fk_artifacts_ai_execution_id_ai_executions",
        ),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    job_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    job_item_id: Mapped[int | None] = mapped_column(bigint_type(), nullable=True)
    ai_execution_id: Mapped[int | None] = mapped_column(bigint_type(), nullable=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    artifact_type: Mapped[str] = mapped_column(String(64), nullable=False)
    storage_provider: Mapped[str] = mapped_column(String(64), nullable=False)
    object_key: Mapped[str] = mapped_column(String(256), nullable=False)
    file_name: Mapped[str | None] = mapped_column(String(256), nullable=True)
    mime_type: Mapped[str | None] = mapped_column(String(128), nullable=True)
    size_bytes: Mapped[int | None] = mapped_column(bigint_type(), nullable=True)
    result_summary: Mapped[str] = mapped_column(Text, nullable=False, default="")
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    file_cleanup_requested_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    file_cleaned_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_artifacts_job_item_id_created_at", ArtifactRecord.job_item_id, ArtifactRecord.created_at)
