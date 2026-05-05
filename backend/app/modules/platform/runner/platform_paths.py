from __future__ import annotations

from pathlib import Path

from app.modules.platform.application.services.platform_file_storage import resolve_platform_storage_path


def resolve_server_workspace_root(input_payload: dict[str, object], *, step_type: str) -> Path:
    object_key = str(input_payload.get("server_workspace_object_key", "")).strip()
    if object_key:
        project_root = resolve_platform_storage_path(object_key)
    else:
        root_text = str(input_payload.get("server_workspace_root", "")).strip()
        if not root_text:
            raise ValueError(f"{step_type} requires server_workspace_root")
        project_root = Path(root_text)
    if not project_root.exists() or not project_root.is_dir():
        raise ValueError(f"server workspace root does not exist: {project_root}")
    return project_root


def resolve_optional_uploaded_asset_path(input_payload: dict[str, object]) -> Path | None:
    object_key = str(input_payload.get("uploaded_asset_object_key", "")).strip()
    if object_key:
        return resolve_platform_storage_path(object_key)
    value = str(input_payload.get("uploaded_asset_path", "")).strip()
    return Path(value) if value else None
