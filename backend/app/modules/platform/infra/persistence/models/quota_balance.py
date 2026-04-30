from __future__ import annotations

from sqlalchemy import ForeignKey, Integer, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, str_enum_type
from .quota_account import QuotaAccountStatus


class QuotaBalanceRecord(TimestampMixin, Base):
    __tablename__ = "quota_balances"
    __table_args__ = (
        UniqueConstraint("user_id", name="uq_quota_balances_user_id"),
        UniqueConstraint("quota_account_id", name="uq_quota_balances_account_id"),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    quota_account_id: Mapped[int] = mapped_column(ForeignKey("quota_accounts.id"), nullable=False)
    total_limit: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    used_amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    refunded_amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    adjusted_amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    status: Mapped[QuotaAccountStatus] = mapped_column(
        str_enum_type(QuotaAccountStatus, "quota_balance_status"),
        nullable=False,
        default=QuotaAccountStatus.ACTIVE,
    )
