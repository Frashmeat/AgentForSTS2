from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import UploadedAssetRecord


class UploadedAssetRepository(ABC):
    @abstractmethod
    def create(self, asset: UploadedAssetRecord) -> UploadedAssetRecord: ...

    @abstractmethod
    def find_by_ref_for_user(self, user_id: int, uploaded_asset_ref: str) -> UploadedAssetRecord | None: ...
