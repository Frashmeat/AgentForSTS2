from __future__ import annotations

from sqlalchemy import DateTime, ForeignKey, Index, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type


class CredentialHealthCheckRecord(TimestampMixin, Base):
    __tablename__ = "credential_health_checks"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    server_credential_id: Mapped[int] = mapped_column(
        bigint_type(),
        ForeignKey("server_credentials.id", ondelete="CASCADE"),
        nullable=False,
    )
    trigger_source: Mapped[str] = mapped_column(String(32), nullable=False)
    status: Mapped[str] = mapped_column(String(32), nullable=False)
    error_code: Mapped[str] = mapped_column(String(64), nullable=False, default="")
    error_message: Mapped[str] = mapped_column(Text, nullable=False, default="")
    latency_ms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    checked_at: Mapped[object] = mapped_column(DateTime(timezone=True), nullable=False)


Index(
    "ix_credential_health_checks_credential_checked_at",
    CredentialHealthCheckRecord.server_credential_id,
    CredentialHealthCheckRecord.checked_at.desc(),
)
