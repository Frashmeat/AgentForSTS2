from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4


_LOCK_VERSION = "v2"
_DEFAULT_DEPLOY_LOCK_LEASE_SECONDS = 300
_DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS = 30


@dataclass(slots=True)
class ServerDeployTargetLockHolder:
    lock_version: str
    job_id: int
    job_item_id: int
    user_id: int
    owner_scope: str
    locked_at: str
    expires_at: str
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
        last_successful_deploy: dict[str, object] | None = None,
        recovery_context: dict[str, object] | None = None,
    ) -> None:
        super().__init__(message)
        self.project_name = project_name
        self.server_project_ref = server_project_ref
        self.source_workspace_root = source_workspace_root
        self.current_holder = current_holder
        self.last_successful_deploy = dict(last_successful_deploy or {}) or None
        self.recovery_context = dict(recovery_context or {}) or None

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
                "expires_at": self.current_holder.expires_at,
                "lock_version": self.current_holder.lock_version,
                "resource_key": self.current_holder.resource_key,
                "project_name": self.current_holder.project_name,
                "server_project_ref": self.current_holder.server_project_ref,
                "source_workspace_root": self.current_holder.source_workspace_root,
            }
        if self.last_successful_deploy is not None:
            payload["last_successful_deploy"] = dict(self.last_successful_deploy)
        if self.recovery_context is not None:
            payload["recovery_context"] = dict(self.recovery_context)
        return payload


class ServerDeployTargetLockService:
    def __init__(
        self,
        storage_root: Path | None = None,
        *,
        lease_seconds: int = _DEFAULT_DEPLOY_LOCK_LEASE_SECONDS,
        recovery_claim_lease_seconds: int = _DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS,
    ) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-deploy-locks"
        self.lease_seconds = max(int(lease_seconds or 0), 1)
        self.recovery_claim_lease_seconds = max(int(recovery_claim_lease_seconds or 0), 1)

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
        claim_path = self._claim_path(lock_path)

        while True:
            now = datetime.now(UTC)
            self._cleanup_stale_claim_file(claim_path, now)
            if claim_path.exists() and not lock_path.exists():
                raise ServerDeployTargetBusyError(
                    "server deploy target is busy",
                    project_name=normalized_project_name,
                    server_project_ref=str(server_project_ref).strip(),
                    source_workspace_root=str(source_workspace_root).strip(),
                )
            holder = self._build_holder(
                project_name=normalized_project_name,
                job_id=job_id,
                job_item_id=job_item_id,
                user_id=user_id,
                owner_scope=owner_scope,
                server_project_ref=str(server_project_ref).strip(),
                source_workspace_root=str(source_workspace_root).strip(),
                now=now,
            )
            created = self._try_create_lock_file(lock_path, holder)
            if created:
                return ServerDeployTargetLockHandle(
                    project_name=normalized_project_name,
                    owner_scope=owner_scope,
                    lock_path=lock_path,
                    holder=holder,
                )

            current_holder = self._read_holder(lock_path, normalized_project_name)
            if current_holder is not None and not self._is_holder_expired(current_holder, now):
                raise ServerDeployTargetBusyError(
                    "server deploy target is busy",
                    project_name=normalized_project_name,
                    server_project_ref=str(server_project_ref).strip(),
                    source_workspace_root=str(source_workspace_root).strip(),
                    current_holder=current_holder,
                )

            claim_created = self._try_create_claim_file(claim_path, now)
            if not claim_created:
                refreshed_holder = self._read_holder(lock_path, normalized_project_name)
                raise ServerDeployTargetBusyError(
                    "server deploy target is busy",
                    project_name=normalized_project_name,
                    server_project_ref=str(server_project_ref).strip(),
                    source_workspace_root=str(source_workspace_root).strip(),
                    current_holder=refreshed_holder,
                )

            try:
                latest_holder = self._read_holder(lock_path, normalized_project_name)
                if latest_holder is not None and not self._is_holder_expired(latest_holder, now):
                    raise ServerDeployTargetBusyError(
                        "server deploy target is busy",
                        project_name=normalized_project_name,
                        server_project_ref=str(server_project_ref).strip(),
                        source_workspace_root=str(source_workspace_root).strip(),
                        current_holder=latest_holder,
                    )
                lock_path.unlink(missing_ok=True)
                recovered = self._try_create_lock_file(lock_path, holder)
                if recovered:
                    return ServerDeployTargetLockHandle(
                        project_name=normalized_project_name,
                        owner_scope=owner_scope,
                        lock_path=lock_path,
                        holder=holder,
                    )
            finally:
                claim_path.unlink(missing_ok=True)

    def release_write_lock(self, handle: ServerDeployTargetLockHandle) -> None:
        handle.lock_path.unlink(missing_ok=True)

    def _build_holder(
        self,
        *,
        project_name: str,
        job_id: int,
        job_item_id: int,
        user_id: int,
        owner_scope: str,
        server_project_ref: str,
        source_workspace_root: str,
        now: datetime,
    ) -> ServerDeployTargetLockHolder:
        locked_at = now.isoformat()
        expires_at = self._build_expires_at(now).isoformat()
        return ServerDeployTargetLockHolder(
            lock_version=_LOCK_VERSION,
            job_id=job_id,
            job_item_id=job_item_id,
            user_id=user_id,
            owner_scope=owner_scope,
            locked_at=locked_at,
            expires_at=expires_at,
            resource_key=project_name,
            project_name=project_name,
            server_project_ref=server_project_ref,
            source_workspace_root=source_workspace_root,
        )

    def _try_create_lock_file(self, lock_path: Path, holder: ServerDeployTargetLockHolder) -> bool:
        try:
            fd = os.open(str(lock_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(
                    {
                        "lock_version": holder.lock_version,
                        "project_name": holder.project_name,
                        "job_id": holder.job_id,
                        "job_item_id": holder.job_item_id,
                        "user_id": holder.user_id,
                        "owner_scope": holder.owner_scope,
                        "locked_at": holder.locked_at,
                        "expires_at": holder.expires_at,
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

    def _is_holder_expired(self, holder: ServerDeployTargetLockHolder, now: datetime) -> bool:
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
    def _token_from_project_name(project_name: str) -> str:
        return hashlib.sha1(project_name.encode("utf-8")).hexdigest()

    @staticmethod
    def _read_holder(lock_path: Path, project_name: str) -> ServerDeployTargetLockHolder | None:
        try:
            payload = json.loads(lock_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            locked_at = str(payload.get("locked_at", "")).strip()
            return ServerDeployTargetLockHolder(
                lock_version=str(payload.get("lock_version", "v1")).strip() or "v1",
                job_id=int(payload.get("job_id", 0) or 0),
                job_item_id=int(payload.get("job_item_id", 0) or 0),
                user_id=int(payload.get("user_id", 0) or 0),
                owner_scope=str(payload.get("owner_scope", "")).strip() or "deploy_target_write",
                locked_at=locked_at,
                expires_at=str(payload.get("expires_at", "")).strip(),
                resource_key=str(payload.get("resource_key", project_name)).strip() or project_name,
                project_name=str(payload.get("project_name", project_name)).strip() or project_name,
                server_project_ref=str(payload.get("server_project_ref", "")).strip(),
                source_workspace_root=str(payload.get("source_workspace_root", "")).strip(),
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
