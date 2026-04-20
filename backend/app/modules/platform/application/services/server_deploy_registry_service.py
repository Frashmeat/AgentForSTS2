from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path


@dataclass(slots=True)
class ServerDeployRegistration:
    schema_version: str
    project_name: str
    job_id: int
    job_item_id: int
    user_id: int
    server_project_ref: str
    source_workspace_root: str
    deployed_at: str
    deployed_to: str
    entrypoint: str
    file_names: list[str]


class ServerDeployRegistryService:
    def __init__(self, metadata_file_name: str = ".server-deploy.json") -> None:
        self.metadata_file_name = str(metadata_file_name).strip() or ".server-deploy.json"

    def resolve_metadata_path(self, target_dir: Path) -> Path:
        return Path(target_dir) / self.metadata_file_name

    def write_registration(
        self,
        *,
        target_dir: Path,
        project_name: str,
        job_id: int,
        job_item_id: int,
        user_id: int,
        server_project_ref: str,
        source_workspace_root: str,
        deployed_to: str,
        entrypoint: str,
        file_names: list[str],
        deployed_at: datetime | None = None,
    ) -> Path:
        normalized_target_dir = Path(target_dir)
        normalized_target_dir.mkdir(parents=True, exist_ok=True)
        registration = ServerDeployRegistration(
            schema_version="v1",
            project_name=str(project_name).strip(),
            job_id=int(job_id or 0),
            job_item_id=int(job_item_id or 0),
            user_id=int(user_id or 0),
            server_project_ref=str(server_project_ref).strip(),
            source_workspace_root=str(source_workspace_root).strip(),
            deployed_at=(deployed_at or datetime.now(UTC)).isoformat(),
            deployed_to=str(deployed_to).strip(),
            entrypoint=str(entrypoint).strip(),
            file_names=[str(name).strip() for name in file_names if str(name).strip()],
        )
        metadata_path = self.resolve_metadata_path(normalized_target_dir)
        temp_path = metadata_path.with_suffix(f"{metadata_path.suffix}.tmp")
        temp_path.write_text(
            json.dumps(asdict(registration), ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        temp_path.replace(metadata_path)
        return metadata_path

    def read_registration(self, target_dir: Path) -> ServerDeployRegistration | None:
        metadata_path = self.resolve_metadata_path(target_dir)
        try:
            payload = json.loads(metadata_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            return ServerDeployRegistration(
                schema_version=str(payload.get("schema_version", "v1")).strip() or "v1",
                project_name=str(payload.get("project_name", "")).strip(),
                job_id=int(payload.get("job_id", 0) or 0),
                job_item_id=int(payload.get("job_item_id", 0) or 0),
                user_id=int(payload.get("user_id", 0) or 0),
                server_project_ref=str(payload.get("server_project_ref", "")).strip(),
                source_workspace_root=str(payload.get("source_workspace_root", "")).strip(),
                deployed_at=str(payload.get("deployed_at", "")).strip(),
                deployed_to=str(payload.get("deployed_to", "")).strip(),
                entrypoint=str(payload.get("entrypoint", "")).strip(),
                file_names=[str(name).strip() for name in payload.get("file_names", []) if str(name).strip()],
            )
        except Exception:
            return None
