from __future__ import annotations

from sqlalchemy import DateTime, ForeignKey, Index, String
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base
from app.modules.platform.infra.persistence.models._common import TimestampMixin, bigint_type


class EmailVerificationRecord(TimestampMixin, Base):
    __tablename__ = "email_verifications"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(
        bigint_type(),
        ForeignKey("users.user_id", ondelete="CASCADE"),
        nullable=False,
    )
    purpose: Mapped[str] = mapped_column(String(32), nullable=False, default="verify_email")
    code: Mapped[str] = mapped_column(String(128), nullable=False, unique=True)
    email: Mapped[str] = mapped_column(String(255), nullable=False)
    expires_at: Mapped[object] = mapped_column(DateTime(timezone=True), nullable=False)
    consumed_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_email_verifications_user_id", EmailVerificationRecord.user_id)
Index("ix_email_verifications_purpose", EmailVerificationRecord.purpose)
