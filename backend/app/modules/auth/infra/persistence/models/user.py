from __future__ import annotations

from sqlalchemy import Boolean, DateTime, Index, String
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base
from app.modules.platform.infra.persistence.models._common import TimestampMixin, bigint_type


class UserRecord(TimestampMixin, Base):
    __tablename__ = "users"

    user_id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    username: Mapped[str] = mapped_column(String(64), nullable=False, unique=True)
    email: Mapped[str] = mapped_column(String(255), nullable=False, unique=True)
    password_hash: Mapped[str] = mapped_column(String(255), nullable=False)
    email_verified: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    email_verified_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_users_username", UserRecord.username)
Index("ix_users_email", UserRecord.email)
