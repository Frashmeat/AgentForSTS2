from __future__ import annotations

from enum import StrEnum

from sqlalchemy import CheckConstraint, DateTime, ForeignKey, Integer, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, str_enum_type


class QuotaBucketType(StrEnum):
    DAILY = "daily"
    WEEKLY = "weekly"


class QuotaBucketRecord(TimestampMixin, Base):
    __tablename__ = "quota_buckets"
    __table_args__ = (
        CheckConstraint("period_end > period_start", name="quota_buckets_period"),
        UniqueConstraint(
            "quota_account_id",
            "bucket_type",
            "period_start",
            "period_end",
            name="uq_quota_buckets_period",
        ),
    )

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    quota_account_id: Mapped[int] = mapped_column(ForeignKey("quota_accounts.id"), nullable=False)
    bucket_type: Mapped[QuotaBucketType] = mapped_column(
        str_enum_type(QuotaBucketType, "quota_bucket_type"),
        nullable=False,
    )
    period_start: Mapped[object] = mapped_column(DateTime(timezone=True), nullable=False)
    period_end: Mapped[object] = mapped_column(DateTime(timezone=True), nullable=False)
    quota_limit: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    used_amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    refunded_amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
