from __future__ import annotations

from enum import StrEnum

from sqlalchemy import JSON, BigInteger, DateTime, Enum as SqlEnum, Integer, func
from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.orm import Mapped, mapped_column


def str_enum_type(enum_cls: type[StrEnum], name: str) -> SqlEnum:
    return SqlEnum(
        enum_cls,
        name=name,
        native_enum=False,
        create_constraint=True,
        validate_strings=True,
        values_callable=lambda members: [member.value for member in members],
    )


def json_type() -> JSON:
    return JSON().with_variant(JSONB, "postgresql")


def bigint_type() -> BigInteger:
    return BigInteger().with_variant(Integer, "sqlite")


class TimestampMixin:
    created_at: Mapped[object] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
    )
    updated_at: Mapped[object] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
    )
