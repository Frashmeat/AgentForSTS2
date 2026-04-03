from __future__ import annotations

from enum import StrEnum

from sqlalchemy import UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, str_enum_type


class QuotaAccountStatus(StrEnum):
    ACTIVE = "active"
    SUSPENDED = "suspended"
    CLOSED = "closed"


class QuotaAccountRecord(TimestampMixin, Base):
    __tablename__ = "quota_accounts"
    __table_args__ = (
        UniqueConstraint("user_id", name="uq_quota_accounts_user_id"),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    status: Mapped[QuotaAccountStatus] = mapped_column(
        str_enum_type(QuotaAccountStatus, "quota_account_status"),
        nullable=False,
        default=QuotaAccountStatus.ACTIVE,
    )
