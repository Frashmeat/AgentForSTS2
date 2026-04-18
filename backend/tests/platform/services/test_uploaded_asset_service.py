from __future__ import annotations

import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.uploaded_asset_service import UploadedAssetService


def test_uploaded_asset_service_creates_persisted_reference(tmp_path):
    service = UploadedAssetService(storage_root=tmp_path / "uploads")

    uploaded = service.create_asset(
        user_id=1001,
        file_name="dark-blade.png",
        content_base64=base64.b64encode(b"fake-image-bytes").decode(),
        mime_type="image/png",
    )

    assert uploaded.uploaded_asset_ref.startswith("uploaded-asset:")
    assert uploaded.file_name == "dark-blade.png"
    assert uploaded.mime_type == "image/png"
    assert uploaded.size_bytes == len(b"fake-image-bytes")

    token = uploaded.uploaded_asset_ref.split(":", 1)[1]
    asset_dir = tmp_path / "uploads" / "1001" / token
    assert (asset_dir / "metadata.json").exists() is True
    assert list(asset_dir.glob("content.*"))


def test_uploaded_asset_service_rejects_invalid_base64(tmp_path):
    service = UploadedAssetService(storage_root=tmp_path / "uploads")

    try:
        service.create_asset(
            user_id=1001,
            file_name="dark-blade.png",
            content_base64="not-base64",
            mime_type="image/png",
        )
    except ValueError as error:
        assert str(error) == "uploaded asset content_base64 is invalid"
    else:
        raise AssertionError("expected ValueError when content_base64 is invalid")


def test_uploaded_asset_service_validates_ref_belongs_to_user(tmp_path):
    service = UploadedAssetService(storage_root=tmp_path / "uploads")
    uploaded = service.create_asset(
        user_id=1001,
        file_name="dark-blade.png",
        content_base64=base64.b64encode(b"fake-image-bytes").decode(),
        mime_type="image/png",
    )

    service.ensure_accessible(user_id=1001, uploaded_asset_ref=uploaded.uploaded_asset_ref)

    try:
        service.ensure_accessible(user_id=1002, uploaded_asset_ref=uploaded.uploaded_asset_ref)
    except ValueError as error:
        assert str(error) == f"uploaded asset ref not found for user: {uploaded.uploaded_asset_ref}"
    else:
        raise AssertionError("expected ValueError when uploaded asset ref belongs to another user")
