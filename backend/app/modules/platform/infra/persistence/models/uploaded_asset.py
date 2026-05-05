from __future__ import annotations

from sqlalchemy import DateTime, Index, String
from sqlalchemy.orm import Mapped, mapped_column

from app.shared.infra.db.base import Base

from ._common import TimestampMixin, bigint_type


class UploadedAssetRecord(TimestampMixin, Base):
    __tablename__ = "uploaded_assets"

    id: Mapped[int] = mapped_column(bigint_type(), primary_key=True, autoincrement=True)
    uploaded_asset_ref: Mapped[str] = mapped_column(String(128), nullable=False, unique=True)
    user_id: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    file_name: Mapped[str] = mapped_column(String(256), nullable=False)
    mime_type: Mapped[str] = mapped_column(String(128), nullable=False)
    size_bytes: Mapped[int] = mapped_column(bigint_type(), nullable=False)
    storage_provider: Mapped[str] = mapped_column(String(64), nullable=False)
    object_key: Mapped[str] = mapped_column(String(512), nullable=False)
    sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    deleted_at: Mapped[object | None] = mapped_column(DateTime(timezone=True), nullable=True)


Index("ix_uploaded_assets_user_ref", UploadedAssetRecord.user_id, UploadedAssetRecord.uploaded_asset_ref)
