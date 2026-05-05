from __future__ import annotations

import json
import secrets
from datetime import UTC, datetime
from pathlib import Path

from project_utils import create_project_from_template

from app.modules.platform.contracts.server_workspace import ServerWorkspaceView

from .platform_file_storage import default_platform_storage_root, normalize_object_key


class ServerWorkspaceService:
    def __init__(self, storage_root: Path | None = None) -> None:
        self.storage_root = storage_root or default_platform_storage_root() / "workspaces"

    def create_workspace(self, *, user_id: int, project_name: str) -> ServerWorkspaceView:
        normalized_project_name = str(project_name).strip()
        if not normalized_project_name:
            raise ValueError("server workspace project_name is required")

        token = secrets.token_hex(12)
        server_project_ref = f"server-workspace:{token}"
        created_at = datetime.now(UTC).isoformat()

        user_root = self.storage_root / str(user_id)
        workspace_root = user_root / token
        project_root = create_project_from_template(normalized_project_name, workspace_root)
        metadata_path = workspace_root / "metadata.json"
        metadata_path.write_text(
            json.dumps(
                {
                    "server_project_ref": server_project_ref,
                    "user_id": user_id,
                    "project_name": normalized_project_name,
                    "workspace_root": str(project_root),
                    "workspace_object_key": self._object_key_for_project(
                        user_id=user_id,
                        token=token,
                        project_name=normalized_project_name,
                    ),
                    "created_at": created_at,
                },
                ensure_ascii=False,
                indent=2,
            ),
            encoding="utf-8",
        )

        return ServerWorkspaceView(
            server_project_ref=server_project_ref,
            project_name=normalized_project_name,
            workspace_root=str(project_root),
            created_at=created_at,
        )

    def ensure_accessible(self, *, user_id: int, server_project_ref: str) -> None:
        self.get_workspace(user_id=user_id, server_project_ref=server_project_ref)

    def get_workspace(self, *, user_id: int, server_project_ref: str) -> ServerWorkspaceView:
        token = self._token_from_ref(server_project_ref)
        metadata_path = self.storage_root / str(user_id) / token / "metadata.json"
        if not metadata_path.exists():
            raise ValueError(f"server workspace ref not found for user: {server_project_ref}")

        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        return ServerWorkspaceView(
            server_project_ref=str(metadata.get("server_project_ref", server_project_ref)),
            project_name=str(metadata.get("project_name", "")).strip(),
            workspace_root=str(metadata.get("workspace_root", "")).strip(),
            created_at=str(metadata.get("created_at", "")).strip(),
        )

    @staticmethod
    def _object_key_for_project(*, user_id: int, token: str, project_name: str) -> str:
        return normalize_object_key("workspaces", user_id, token, project_name)

    @staticmethod
    def _token_from_ref(server_project_ref: str) -> str:
        prefix = "server-workspace:"
        value = str(server_project_ref).strip()
        if not value.startswith(prefix) or len(value) <= len(prefix):
            raise ValueError(f"server workspace ref is invalid: {server_project_ref}")
        return value[len(prefix) :]
