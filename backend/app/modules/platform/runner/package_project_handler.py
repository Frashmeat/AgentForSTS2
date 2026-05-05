from __future__ import annotations

import zipfile
from pathlib import Path

from app.modules.platform.application.services.platform_file_storage import (
    PLATFORM_FS_PROVIDER,
    try_object_key_from_platform_path,
)
from app.modules.platform.contracts.runner_contracts import StepExecutionRequest

from .platform_paths import resolve_server_workspace_root

_SOURCE_PACKAGE_SKIP_DIRS = {"bin", "obj", ".godot", ".git", "_source_artifacts"}


def _resolve_project_root(input_payload: dict[str, object]) -> Path:
    return resolve_server_workspace_root(input_payload, step_type="package.project")


def _create_source_project_package(project_root: Path) -> Path:
    artifact_dir = project_root.parent / "_source_artifacts"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    package_path = artifact_dir / f"{project_root.name}.source.zip"
    if package_path.exists():
        package_path.unlink()
    with zipfile.ZipFile(package_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(project_root.rglob("*")):
            if path.is_dir():
                continue
            relative = path.relative_to(project_root)
            if any(part in _SOURCE_PACKAGE_SKIP_DIRS for part in relative.parts):
                continue
            archive.write(path, arcname=str(relative).replace("\\", "/"))
    return package_path


async def execute_package_project_step(request: StepExecutionRequest) -> dict[str, object]:
    project_root = _resolve_project_root(request.input_payload)
    item_name = str(request.input_payload.get("item_name", "")).strip() or project_root.name
    package_path = _create_source_project_package(project_root)
    object_key = try_object_key_from_platform_path(package_path)
    return {
        "text": f"已打包 {item_name} 的服务器项目源码",
        "item_name": item_name,
        "server_workspace_root": str(project_root),
        "artifacts": [
            {
                "artifact_type": "source_project",
                "storage_provider": PLATFORM_FS_PROVIDER if object_key is not None else "server_workspace",
                "object_key": object_key or str(package_path),
                "file_name": package_path.name,
                "mime_type": "application/zip",
                "size_bytes": package_path.stat().st_size,
                "result_summary": "服务器生成项目包",
            }
        ],
    }
