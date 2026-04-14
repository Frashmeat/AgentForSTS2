from __future__ import annotations

from sqlalchemy import ForeignKey, Index
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type


class UserPlatformPreferenceRecord(TimestampMixin, Base):
    __tablename__ = "user_platform_preferences"

    user_id: Mapped[int] = mapped_column(
        bigint_type(),
        ForeignKey("users.user_id", ondelete="CASCADE"),
        primary_key=True,
    )
    default_execution_profile_id: Mapped[int | None] = mapped_column(
        bigint_type(),
        ForeignKey("execution_profiles.id", ondelete="SET NULL"),
        nullable=True,
    )


Index("ix_user_platform_preferences_default_execution_profile_id", UserPlatformPreferenceRecord.default_execution_profile_id)
