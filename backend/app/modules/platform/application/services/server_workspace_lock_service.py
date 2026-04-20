from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path


@dataclass(slots=True)
class ServerWorkspaceLockHolder:
    job_id: int
    job_item_id: int
    user_id: int
    owner_scope: str
    locked_at: str
    resource_key: str


@dataclass(slots=True)
class ServerWorkspaceLockHandle:
    server_project_ref: str
    owner_scope: str
    lock_path: Path
    holder: ServerWorkspaceLockHolder


class ServerWorkspaceBusyError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        server_project_ref: str,
        current_holder: ServerWorkspaceLockHolder | None = None,
    ) -> None:
        super().__init__(message)
        self.server_project_ref = server_project_ref
        self.current_holder = current_holder


class ServerWorkspaceLockService:
    def __init__(self, storage_root: Path | None = None) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-workspace-locks"

    def acquire_write_lock(
        self,
        *,
        server_project_ref: str,
        job_id: int,
        job_item_id: int,
        user_id: int,
        owner_scope: str = "workspace_write",
    ) -> ServerWorkspaceLockHandle:
        token = self._token_from_ref(server_project_ref)
        self.storage_root.mkdir(parents=True, exist_ok=True)
        lock_path = self.storage_root / f"{token}.lock.json"
        holder = ServerWorkspaceLockHolder(
            job_id=job_id,
            job_item_id=job_item_id,
            user_id=user_id,
            owner_scope=owner_scope,
            locked_at=datetime.now(UTC).isoformat(),
            resource_key=server_project_ref,
        )
        try:
            fd = os.open(str(lock_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError as error:
            raise ServerWorkspaceBusyError(
                "server workspace is busy",
                server_project_ref=server_project_ref,
                current_holder=self._read_holder(lock_path, server_project_ref),
            ) from error

        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(
                    {
                        "server_project_ref": server_project_ref,
                        "job_id": holder.job_id,
                        "job_item_id": holder.job_item_id,
                        "user_id": holder.user_id,
                        "owner_scope": holder.owner_scope,
                        "locked_at": holder.locked_at,
                        "resource_key": holder.resource_key,
                    },
                    stream,
                    ensure_ascii=False,
                    indent=2,
                )
        except Exception:
            lock_path.unlink(missing_ok=True)
            raise

        return ServerWorkspaceLockHandle(
            server_project_ref=server_project_ref,
            owner_scope=owner_scope,
            lock_path=lock_path,
            holder=holder,
        )

    def release_write_lock(self, handle: ServerWorkspaceLockHandle) -> None:
        handle.lock_path.unlink(missing_ok=True)

    @staticmethod
    def _token_from_ref(server_project_ref: str) -> str:
        prefix = "server-workspace:"
        value = str(server_project_ref).strip()
        if not value.startswith(prefix) or len(value) <= len(prefix):
            raise ValueError(f"server workspace ref is invalid: {server_project_ref}")
        return value[len(prefix):]

    @staticmethod
    def _read_holder(lock_path: Path, server_project_ref: str) -> ServerWorkspaceLockHolder | None:
        try:
            payload = json.loads(lock_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            return ServerWorkspaceLockHolder(
                job_id=int(payload.get("job_id", 0) or 0),
                job_item_id=int(payload.get("job_item_id", 0) or 0),
                user_id=int(payload.get("user_id", 0) or 0),
                owner_scope=str(payload.get("owner_scope", "")).strip() or "workspace_write",
                locked_at=str(payload.get("locked_at", "")).strip(),
                resource_key=str(payload.get("resource_key", server_project_ref)).strip() or server_project_ref,
            )
        except Exception:
            return None
