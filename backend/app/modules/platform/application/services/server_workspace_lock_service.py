from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

_LOCK_VERSION = "v2"
_DEFAULT_WORKSPACE_LOCK_LEASE_SECONDS = 1800
_DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS = 30


@dataclass(slots=True)
class ServerWorkspaceLockHolder:
    lock_version: str
    job_id: int
    job_item_id: int
    user_id: int
    owner_scope: str
    locked_at: str
    expires_at: str
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
    def __init__(
        self,
        storage_root: Path | None = None,
        *,
        lease_seconds: int = _DEFAULT_WORKSPACE_LOCK_LEASE_SECONDS,
        recovery_claim_lease_seconds: int = _DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS,
    ) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-workspace-locks"
        self.lease_seconds = max(int(lease_seconds or 0), 1)
        self.recovery_claim_lease_seconds = max(int(recovery_claim_lease_seconds or 0), 1)

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
        claim_path = self._claim_path(lock_path)

        while True:
            now = datetime.now(UTC)
            self._cleanup_stale_claim_file(claim_path, now)
            if claim_path.exists() and not lock_path.exists():
                raise ServerWorkspaceBusyError(
                    "server workspace is busy",
                    server_project_ref=server_project_ref,
                )
            holder = self._build_holder(
                server_project_ref=server_project_ref,
                job_id=job_id,
                job_item_id=job_item_id,
                user_id=user_id,
                owner_scope=owner_scope,
                now=now,
            )
            created = self._try_create_lock_file(lock_path, server_project_ref, holder)
            if created:
                return ServerWorkspaceLockHandle(
                    server_project_ref=server_project_ref,
                    owner_scope=owner_scope,
                    lock_path=lock_path,
                    holder=holder,
                )

            current_holder = self._read_holder(lock_path, server_project_ref)
            if current_holder is not None and not self._is_holder_expired(current_holder, now):
                raise ServerWorkspaceBusyError(
                    "server workspace is busy",
                    server_project_ref=server_project_ref,
                    current_holder=current_holder,
                )

            claim_created = self._try_create_claim_file(claim_path, now)
            if not claim_created:
                refreshed_holder = self._read_holder(lock_path, server_project_ref)
                raise ServerWorkspaceBusyError(
                    "server workspace is busy",
                    server_project_ref=server_project_ref,
                    current_holder=refreshed_holder,
                )

            try:
                latest_holder = self._read_holder(lock_path, server_project_ref)
                if latest_holder is not None and not self._is_holder_expired(latest_holder, now):
                    raise ServerWorkspaceBusyError(
                        "server workspace is busy",
                        server_project_ref=server_project_ref,
                        current_holder=latest_holder,
                    )
                lock_path.unlink(missing_ok=True)
                recovered = self._try_create_lock_file(lock_path, server_project_ref, holder)
                if recovered:
                    return ServerWorkspaceLockHandle(
                        server_project_ref=server_project_ref,
                        owner_scope=owner_scope,
                        lock_path=lock_path,
                        holder=holder,
                    )
            finally:
                claim_path.unlink(missing_ok=True)

    def release_write_lock(self, handle: ServerWorkspaceLockHandle) -> None:
        handle.lock_path.unlink(missing_ok=True)

    def _build_holder(
        self,
        *,
        server_project_ref: str,
        job_id: int,
        job_item_id: int,
        user_id: int,
        owner_scope: str,
        now: datetime,
    ) -> ServerWorkspaceLockHolder:
        locked_at = now.isoformat()
        expires_at = self._build_expires_at(now).isoformat()
        return ServerWorkspaceLockHolder(
            lock_version=_LOCK_VERSION,
            job_id=job_id,
            job_item_id=job_item_id,
            user_id=user_id,
            owner_scope=owner_scope,
            locked_at=locked_at,
            expires_at=expires_at,
            resource_key=server_project_ref,
        )

    def _try_create_lock_file(
        self,
        lock_path: Path,
        server_project_ref: str,
        holder: ServerWorkspaceLockHolder,
    ) -> bool:
        try:
            fd = os.open(str(lock_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False

        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(
                    {
                        "lock_version": holder.lock_version,
                        "server_project_ref": server_project_ref,
                        "job_id": holder.job_id,
                        "job_item_id": holder.job_item_id,
                        "user_id": holder.user_id,
                        "owner_scope": holder.owner_scope,
                        "locked_at": holder.locked_at,
                        "expires_at": holder.expires_at,
                        "resource_key": holder.resource_key,
                    },
                    stream,
                    ensure_ascii=False,
                    indent=2,
                )
        except Exception:
            lock_path.unlink(missing_ok=True)
            raise
        return True

    def _try_create_claim_file(self, claim_path: Path, now: datetime) -> bool:
        claim_payload = {
            "claim_version": _LOCK_VERSION,
            "claim_id": uuid4().hex,
            "claimed_at": now.isoformat(),
            "expires_at": (now.timestamp() + self.recovery_claim_lease_seconds),
        }
        try:
            fd = os.open(str(claim_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(claim_payload, stream, ensure_ascii=False, indent=2)
        except Exception:
            claim_path.unlink(missing_ok=True)
            raise
        return True

    def _cleanup_stale_claim_file(self, claim_path: Path, now: datetime) -> None:
        try:
            payload = json.loads(claim_path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return
        except Exception:
            claim_path.unlink(missing_ok=True)
            return
        expires_at = payload.get("expires_at")
        try:
            expires_timestamp = float(expires_at)
        except (TypeError, ValueError):
            claim_path.unlink(missing_ok=True)
            return
        if expires_timestamp <= now.timestamp():
            claim_path.unlink(missing_ok=True)

    def _is_holder_expired(self, holder: ServerWorkspaceLockHolder, now: datetime) -> bool:
        expires_at = self._parse_datetime(holder.expires_at)
        if expires_at is None:
            locked_at = self._parse_datetime(holder.locked_at)
            if locked_at is None:
                return True
            expires_at = self._build_expires_at(locked_at)
        return expires_at <= now

    def _build_expires_at(self, locked_at: datetime) -> datetime:
        return datetime.fromtimestamp(locked_at.timestamp() + self.lease_seconds, tz=UTC)

    @staticmethod
    def _claim_path(lock_path: Path) -> Path:
        return lock_path.with_suffix(".recovering.json")

    @staticmethod
    def _token_from_ref(server_project_ref: str) -> str:
        prefix = "server-workspace:"
        value = str(server_project_ref).strip()
        if not value.startswith(prefix) or len(value) <= len(prefix):
            raise ValueError(f"server workspace ref is invalid: {server_project_ref}")
        return value[len(prefix) :]

    @staticmethod
    def _read_holder(lock_path: Path, server_project_ref: str) -> ServerWorkspaceLockHolder | None:
        try:
            payload = json.loads(lock_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            locked_at = str(payload.get("locked_at", "")).strip()
            return ServerWorkspaceLockHolder(
                lock_version=str(payload.get("lock_version", "v1")).strip() or "v1",
                job_id=int(payload.get("job_id", 0) or 0),
                job_item_id=int(payload.get("job_item_id", 0) or 0),
                user_id=int(payload.get("user_id", 0) or 0),
                owner_scope=str(payload.get("owner_scope", "")).strip() or "workspace_write",
                locked_at=locked_at,
                expires_at=str(payload.get("expires_at", "")).strip(),
                resource_key=str(payload.get("resource_key", server_project_ref)).strip() or server_project_ref,
            )
        except Exception:
            return None

    @staticmethod
    def _parse_datetime(value: str) -> datetime | None:
        text = str(value).strip()
        if not text:
            return None
        try:
            parsed = datetime.fromisoformat(text)
        except ValueError:
            return None
        if parsed.tzinfo is None:
            return parsed.replace(tzinfo=UTC)
        return parsed.astimezone(UTC)
