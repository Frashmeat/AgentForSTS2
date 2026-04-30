from __future__ import annotations

from enum import StrEnum

from sqlalchemy import DateTime, ForeignKey, Integer, String, Text, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type, str_enum_type


class ChargeStatus(StrEnum):
    RESERVED = "reserved"
    CAPTURED = "captured"
    REFUNDED = "refunded"


class ExecutionChargeRecord(TimestampMixin, Base):
    __tablename__ = "execution_charges"
    __table_args__ = (UniqueConstraint("ai_execution_id", name="uq_execution_charges_ai_execution_id"),)

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    ai_execution_id: Mapped[int] = mapped_column(ForeignKey("ai_executions.id"), nullable=False)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    charge_status: Mapped[ChargeStatus] = mapped_column(
        str_enum_type(ChargeStatus, "charge_status"),
        nullable=False,
        default=ChargeStatus.RESERVED,
    )
    charge_unit: Mapped[str] = mapped_column(String(32), nullable=False, default="execution")
    charge_amount: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    refund_reason: Mapped[str] = mapped_column(Text, nullable=False, default="")
    reserved_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    captured_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    refunded_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
    archived_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)
