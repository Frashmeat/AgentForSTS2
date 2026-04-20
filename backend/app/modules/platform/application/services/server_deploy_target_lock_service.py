from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path


@dataclass(slots=True)
class ServerDeployTargetLockHolder:
    job_id: int
    job_item_id: int
    user_id: int
    owner_scope: str
    locked_at: str
    resource_key: str
    project_name: str
    server_project_ref: str
    source_workspace_root: str


@dataclass(slots=True)
class ServerDeployTargetLockHandle:
    project_name: str
    owner_scope: str
    lock_path: Path
    holder: ServerDeployTargetLockHolder


class ServerDeployTargetBusyError(RuntimeError):
    reason_code = "server_deploy_target_busy"
    resource_type = "server_deploy_target"

    def __init__(
        self,
        message: str,
        *,
        project_name: str,
        server_project_ref: str = "",
        source_workspace_root: str = "",
        current_holder: ServerDeployTargetLockHolder | None = None,
    ) -> None:
        super().__init__(message)
        self.project_name = project_name
        self.server_project_ref = server_project_ref
        self.source_workspace_root = source_workspace_root
        self.current_holder = current_holder

    def to_error_payload(self) -> dict[str, object]:
        payload = {
            "reason_code": self.reason_code,
            "reason_message": str(self),
            "resource_type": self.resource_type,
            "resource_key": self.project_name,
            "project_name": self.project_name,
            "server_project_ref": self.server_project_ref,
            "source_workspace_root": self.source_workspace_root,
        }
        if self.current_holder is not None:
            payload["current_holder"] = {
                "job_id": self.current_holder.job_id,
                "job_item_id": self.current_holder.job_item_id,
                "user_id": self.current_holder.user_id,
                "owner_scope": self.current_holder.owner_scope,
                "locked_at": self.current_holder.locked_at,
                "resource_key": self.current_holder.resource_key,
                "project_name": self.current_holder.project_name,
                "server_project_ref": self.current_holder.server_project_ref,
                "source_workspace_root": self.current_holder.source_workspace_root,
            }
        return payload


class ServerDeployTargetLockService:
    def __init__(self, storage_root: Path | None = None) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-deploy-locks"

    def acquire_write_lock(
        self,
        *,
        project_name: str,
        job_id: int,
        job_item_id: int,
        user_id: int,
        server_project_ref: str = "",
        source_workspace_root: str = "",
        owner_scope: str = "deploy_target_write",
    ) -> ServerDeployTargetLockHandle:
        normalized_project_name = str(project_name).strip()
        if not normalized_project_name:
            raise ValueError("project_name is required")

        self.storage_root.mkdir(parents=True, exist_ok=True)
        lock_path = self.storage_root / f"{self._token_from_project_name(normalized_project_name)}.lock.json"
        holder = ServerDeployTargetLockHolder(
            job_id=job_id,
            job_item_id=job_item_id,
            user_id=user_id,
            owner_scope=owner_scope,
            locked_at=datetime.now(UTC).isoformat(),
            resource_key=normalized_project_name,
            project_name=normalized_project_name,
            server_project_ref=str(server_project_ref).strip(),
            source_workspace_root=str(source_workspace_root).strip(),
        )
        try:
            fd = os.open(str(lock_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError as error:
            raise ServerDeployTargetBusyError(
                "server deploy target is busy",
                project_name=normalized_project_name,
                server_project_ref=str(server_project_ref).strip(),
                source_workspace_root=str(source_workspace_root).strip(),
                current_holder=self._read_holder(lock_path, normalized_project_name),
            ) from error

        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(
                    {
                        "project_name": holder.project_name,
                        "job_id": holder.job_id,
                        "job_item_id": holder.job_item_id,
                        "user_id": holder.user_id,
                        "owner_scope": holder.owner_scope,
                        "locked_at": holder.locked_at,
                        "resource_key": holder.resource_key,
                        "server_project_ref": holder.server_project_ref,
                        "source_workspace_root": holder.source_workspace_root,
                    },
                    stream,
                    ensure_ascii=False,
                    indent=2,
                )
        except Exception:
            lock_path.unlink(missing_ok=True)
            raise

        return ServerDeployTargetLockHandle(
            project_name=normalized_project_name,
            owner_scope=owner_scope,
            lock_path=lock_path,
            holder=holder,
        )

    def release_write_lock(self, handle: ServerDeployTargetLockHandle) -> None:
        handle.lock_path.unlink(missing_ok=True)

    @staticmethod
    def _token_from_project_name(project_name: str) -> str:
        return hashlib.sha1(project_name.encode("utf-8")).hexdigest()

    @staticmethod
    def _read_holder(lock_path: Path, project_name: str) -> ServerDeployTargetLockHolder | None:
        try:
            payload = json.loads(lock_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            return ServerDeployTargetLockHolder(
                job_id=int(payload.get("job_id", 0) or 0),
                job_item_id=int(payload.get("job_item_id", 0) or 0),
                user_id=int(payload.get("user_id", 0) or 0),
                owner_scope=str(payload.get("owner_scope", "")).strip() or "deploy_target_write",
                locked_at=str(payload.get("locked_at", "")).strip(),
                resource_key=str(payload.get("resource_key", project_name)).strip() or project_name,
                project_name=str(payload.get("project_name", project_name)).strip() or project_name,
                server_project_ref=str(payload.get("server_project_ref", "")).strip(),
                source_workspace_root=str(payload.get("source_workspace_root", "")).strip(),
            )
        except Exception:
            return None
