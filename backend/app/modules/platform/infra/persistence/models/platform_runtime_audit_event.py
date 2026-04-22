from __future__ import annotations

from sqlalchemy import DateTime, Index, String, text
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, json_type


class PlatformRuntimeAuditEventRecord(TimestampMixin, Base):
    __tablename__ = "platform_runtime_audit_events"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    event_type: Mapped[str] = mapped_column(String(128), nullable=False)
    event_payload: Mapped[dict[str, object]] = mapped_column(
        json_type(),
        nullable=False,
        server_default=text("'{}'"),
        default=dict,
    )
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index(
    "ix_platform_runtime_audit_events_created_at",
    PlatformRuntimeAuditEventRecord.created_at,
)
