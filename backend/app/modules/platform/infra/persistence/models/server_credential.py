from __future__ import annotations

from sqlalchemy import Boolean, DateTime, ForeignKey, Index, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type


class ServerCredentialRecord(TimestampMixin, Base):
    __tablename__ = "server_credentials"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    execution_profile_id: Mapped[int] = mapped_column(
        bigint_type(),
        ForeignKey("execution_profiles.id", ondelete="CASCADE"),
        nullable=False,
    )
    provider: Mapped[str] = mapped_column(String(64), nullable=False)
    auth_type: Mapped[str] = mapped_column(String(32), nullable=False)
    credential_ciphertext: Mapped[str] = mapped_column(Text, nullable=False)
    secret_ciphertext: Mapped[str | None] = mapped_column(Text, nullable=True)
    base_url: Mapped[str] = mapped_column(String(255), nullable=False, default="")
    label: Mapped[str] = mapped_column(String(128), nullable=False, default="")
    priority: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    enabled: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    health_status: Mapped[str] = mapped_column(String(32), nullable=False, default="healthy")
    last_checked_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    last_error_code: Mapped[str] = mapped_column(String(64), nullable=False, default="")
    last_error_message: Mapped[str] = mapped_column(Text, nullable=False, default="")


Index(
    "ix_server_credentials_profile_enabled_priority",
    ServerCredentialRecord.execution_profile_id,
    ServerCredentialRecord.enabled,
    ServerCredentialRecord.priority,
)
