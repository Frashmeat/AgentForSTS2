from __future__ import annotations

from dataclasses import dataclass

from ._model import ModelBase


@dataclass(slots=True)
class UploadAssetCommand(ModelBase):
    file_name: str
    content_base64: str
    mime_type: str = ""


@dataclass(slots=True)
class UploadedAssetView(ModelBase):
    uploaded_asset_ref: str
    file_name: str
    mime_type: str
    size_bytes: int
    created_at: str

