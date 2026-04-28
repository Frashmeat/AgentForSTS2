from __future__ import annotations

from enum import StrEnum

from sqlalchemy import DateTime, ForeignKey, Index, Integer, String
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, str_enum_type


class LedgerType(StrEnum):
    RESERVE = "reserve"
    CAPTURE = "capture"
    REFUND = "refund"
    ADMIN_GRANT = "admin_grant"
    ADMIN_DEDUCT = "admin_deduct"


class UsageLedgerRecord(TimestampMixin, Base):
    __tablename__ = "usage_ledgers"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    quota_account_id: Mapped[int] = mapped_column(ForeignKey("quota_accounts.id"), nullable=False)
    quota_balance_id: Mapped[int | None] = mapped_column(ForeignKey("quota_balances.id"), nullable=True)
    quota_bucket_id: Mapped[int | None] = mapped_column(ForeignKey("quota_buckets.id"), nullable=True)
    ai_execution_id: Mapped[int | None] = mapped_column(ForeignKey("ai_executions.id"), nullable=True)
    ledger_type: Mapped[LedgerType] = mapped_column(
        str_enum_type(LedgerType, "ledger_type"),
        nullable=False,
    )
    amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    balance_after: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    reason_code: Mapped[str] = mapped_column(String(64), nullable=False, default="")
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_usage_ledgers_user_id_created_at_desc", UsageLedgerRecord.user_id, UsageLedgerRecord.created_at.desc())
