from __future__ import annotations

import base64
import binascii
import json
import secrets
from datetime import UTC, datetime
from pathlib import Path

from app.modules.platform.contracts.uploaded_asset import UploadedAssetView


class UploadedAssetService:
    def __init__(self, storage_root: Path | None = None) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-upload-assets"

    def create_asset(
        self,
        *,
        user_id: int,
        file_name: str,
        content_base64: str,
        mime_type: str = "",
    ) -> UploadedAssetView:
        normalized_file_name = Path(file_name or "uploaded.bin").name or "uploaded.bin"
        normalized_mime_type = mime_type.strip() or "application/octet-stream"

        try:
            content = base64.b64decode(content_base64, validate=True)
        except (binascii.Error, ValueError) as error:
            raise ValueError("uploaded asset content_base64 is invalid") from error

        if not content:
            raise ValueError("uploaded asset content is empty")

        token = secrets.token_hex(12)
        uploaded_asset_ref = f"uploaded-asset:{token}"
        created_at = datetime.now(UTC).isoformat()

        asset_dir = self.storage_root / str(user_id) / token
        asset_dir.mkdir(parents=True, exist_ok=True)
        content_path = asset_dir / f"content{Path(normalized_file_name).suffix or '.bin'}"
        metadata_path = asset_dir / "metadata.json"

        content_path.write_bytes(content)
        metadata_path.write_text(
            json.dumps(
                {
                    "uploaded_asset_ref": uploaded_asset_ref,
                    "user_id": user_id,
                    "file_name": normalized_file_name,
                    "mime_type": normalized_mime_type,
                    "size_bytes": len(content),
                    "created_at": created_at,
                },
                ensure_ascii=False,
                indent=2,
            ),
            encoding="utf-8",
        )

        return UploadedAssetView(
            uploaded_asset_ref=uploaded_asset_ref,
            file_name=normalized_file_name,
            mime_type=normalized_mime_type,
            size_bytes=len(content),
            created_at=created_at,
        )

    def ensure_accessible(self, *, user_id: int, uploaded_asset_ref: str) -> None:
        self.get_asset(user_id=user_id, uploaded_asset_ref=uploaded_asset_ref)

    def get_asset_content_path(self, *, user_id: int, uploaded_asset_ref: str) -> Path:
        token = self._token_from_ref(uploaded_asset_ref)
        asset_dir = self.storage_root / str(user_id) / token
        if not asset_dir.exists():
            raise ValueError(f"uploaded asset ref not found for user: {uploaded_asset_ref}")
        candidates = sorted(path for path in asset_dir.iterdir() if path.name.startswith("content"))
        if not candidates:
            raise ValueError(f"uploaded asset content missing for ref: {uploaded_asset_ref}")
        return candidates[0]

    def get_asset(self, *, user_id: int, uploaded_asset_ref: str) -> UploadedAssetView:
        token = self._token_from_ref(uploaded_asset_ref)
        metadata_path = self.storage_root / str(user_id) / token / "metadata.json"
        if not metadata_path.exists():
            raise ValueError(f"uploaded asset ref not found for user: {uploaded_asset_ref}")

        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        return UploadedAssetView(
            uploaded_asset_ref=str(metadata.get("uploaded_asset_ref", uploaded_asset_ref)),
            file_name=str(metadata.get("file_name", "")).strip(),
            mime_type=str(metadata.get("mime_type", "")).strip(),
            size_bytes=int(metadata.get("size_bytes", 0)),
            created_at=str(metadata.get("created_at", "")).strip(),
        )

    @staticmethod
    def _token_from_ref(uploaded_asset_ref: str) -> str:
        prefix = "uploaded-asset:"
        value = str(uploaded_asset_ref).strip()
        if not value.startswith(prefix) or len(value) <= len(prefix):
            raise ValueError(f"uploaded asset ref is invalid: {uploaded_asset_ref}")
        return value[len(prefix):]
