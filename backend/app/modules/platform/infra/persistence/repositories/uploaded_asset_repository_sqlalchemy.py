from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import UploadedAssetRepository
from app.modules.platform.infra.persistence.models import UploadedAssetRecord


class UploadedAssetRepositorySqlAlchemy(UploadedAssetRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create(self, asset: UploadedAssetRecord) -> UploadedAssetRecord:
        self.session.add(asset)
        self.session.flush()
        return asset

    def find_by_ref_for_user(self, user_id: int, uploaded_asset_ref: str) -> UploadedAssetRecord | None:
        return (
            self.session.query(UploadedAssetRecord)
            .filter(
                UploadedAssetRecord.user_id == user_id,
                UploadedAssetRecord.uploaded_asset_ref == uploaded_asset_ref,
                UploadedAssetRecord.deleted_at.is_(None),
            )
            .one_or_none()
        )
