from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4


_CLAIM_VERSION = "v1"
_DEFAULT_JOB_CLAIM_LEASE_SECONDS = 120
_DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS = 15


@dataclass(slots=True)
class ServerQueuedJobClaimHolder:
    claim_version: str
    job_id: int
    owner_scope: str
    claimed_at: str
    expires_at: str


@dataclass(slots=True)
class ServerQueuedJobClaimHandle:
    job_id: int
    claim_path: Path
    holder: ServerQueuedJobClaimHolder


class ServerQueuedJobClaimBusyError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        job_id: int,
        current_holder: ServerQueuedJobClaimHolder | None = None,
    ) -> None:
        super().__init__(message)
        self.job_id = job_id
        self.current_holder = current_holder


class ServerQueuedJobClaimService:
    def __init__(
        self,
        storage_root: Path | None = None,
        *,
        lease_seconds: int = _DEFAULT_JOB_CLAIM_LEASE_SECONDS,
        recovery_claim_lease_seconds: int = _DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS,
    ) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-queued-job-claims"
        self.lease_seconds = max(int(lease_seconds or 0), 1)
        self.recovery_claim_lease_seconds = max(int(recovery_claim_lease_seconds or 0), 1)

    def acquire_claim(self, *, job_id: int, owner_scope: str) -> ServerQueuedJobClaimHandle:
        normalized_job_id = int(job_id or 0)
        if normalized_job_id <= 0:
            raise ValueError("job_id is required")

        self.storage_root.mkdir(parents=True, exist_ok=True)
        claim_path = self.storage_root / f"{normalized_job_id}.claim.json"
        recovering_path = self._recovering_path(claim_path)

        while True:
            now = datetime.now(UTC)
            self._cleanup_stale_recovering_file(recovering_path, now)
            holder = self._build_holder(job_id=normalized_job_id, owner_scope=owner_scope, now=now)
            created = self._try_create_claim_file(claim_path, holder)
            if created:
                return ServerQueuedJobClaimHandle(job_id=normalized_job_id, claim_path=claim_path, holder=holder)

            current_holder = self._read_holder(claim_path, normalized_job_id)
            if current_holder is not None and not self._is_holder_expired(current_holder, now):
                raise ServerQueuedJobClaimBusyError(
                    "queued job claim is busy",
                    job_id=normalized_job_id,
                    current_holder=current_holder,
                )

            recovering_created = self._try_create_recovering_file(recovering_path, now)
            if not recovering_created:
                refreshed_holder = self._read_holder(claim_path, normalized_job_id)
                raise ServerQueuedJobClaimBusyError(
                    "queued job claim is busy",
                    job_id=normalized_job_id,
                    current_holder=refreshed_holder,
                )

            try:
                latest_holder = self._read_holder(claim_path, normalized_job_id)
                if latest_holder is not None and not self._is_holder_expired(latest_holder, now):
                    raise ServerQueuedJobClaimBusyError(
                        "queued job claim is busy",
                        job_id=normalized_job_id,
                        current_holder=latest_holder,
                    )
                claim_path.unlink(missing_ok=True)
                recovered = self._try_create_claim_file(claim_path, holder)
                if recovered:
                    return ServerQueuedJobClaimHandle(job_id=normalized_job_id, claim_path=claim_path, holder=holder)
            finally:
                recovering_path.unlink(missing_ok=True)

    def release_claim(self, handle: ServerQueuedJobClaimHandle) -> None:
        handle.claim_path.unlink(missing_ok=True)

    def _build_holder(self, *, job_id: int, owner_scope: str, now: datetime) -> ServerQueuedJobClaimHolder:
        claimed_at = now.isoformat()
        expires_at = datetime.fromtimestamp(now.timestamp() + self.lease_seconds, tz=UTC).isoformat()
        return ServerQueuedJobClaimHolder(
            claim_version=_CLAIM_VERSION,
            job_id=job_id,
            owner_scope=str(owner_scope).strip() or "job_start",
            claimed_at=claimed_at,
            expires_at=expires_at,
        )

    def _try_create_claim_file(self, claim_path: Path, holder: ServerQueuedJobClaimHolder) -> bool:
        try:
            fd = os.open(str(claim_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(
                    {
                        "claim_version": holder.claim_version,
                        "job_id": holder.job_id,
                        "owner_scope": holder.owner_scope,
                        "claimed_at": holder.claimed_at,
                        "expires_at": holder.expires_at,
                    },
                    stream,
                    ensure_ascii=False,
                    indent=2,
                )
        except Exception:
            claim_path.unlink(missing_ok=True)
            raise
        return True

    def _try_create_recovering_file(self, recovering_path: Path, now: datetime) -> bool:
        payload = {
            "claim_version": _CLAIM_VERSION,
            "recovering_id": uuid4().hex,
            "claimed_at": now.isoformat(),
            "expires_at": now.timestamp() + self.recovery_claim_lease_seconds,
        }
        try:
            fd = os.open(str(recovering_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(payload, stream, ensure_ascii=False, indent=2)
        except Exception:
            recovering_path.unlink(missing_ok=True)
            raise
        return True

    def _cleanup_stale_recovering_file(self, recovering_path: Path, now: datetime) -> None:
        try:
            payload = json.loads(recovering_path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return
        except Exception:
            recovering_path.unlink(missing_ok=True)
            return
        try:
            expires_timestamp = float(payload.get("expires_at"))
        except (TypeError, ValueError):
            recovering_path.unlink(missing_ok=True)
            return
        if expires_timestamp <= now.timestamp():
            recovering_path.unlink(missing_ok=True)

    def _is_holder_expired(self, holder: ServerQueuedJobClaimHolder, now: datetime) -> bool:
        expires_at = self._parse_datetime(holder.expires_at)
        if expires_at is None:
            return True
        return expires_at <= now

    @staticmethod
    def _recovering_path(claim_path: Path) -> Path:
        return claim_path.with_suffix(".recovering.json")

    @staticmethod
    def _read_holder(claim_path: Path, job_id: int) -> ServerQueuedJobClaimHolder | None:
        try:
            payload = json.loads(claim_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            return ServerQueuedJobClaimHolder(
                claim_version=str(payload.get("claim_version", "v1")).strip() or "v1",
                job_id=int(payload.get("job_id", job_id) or job_id),
                owner_scope=str(payload.get("owner_scope", "")).strip() or "job_start",
                claimed_at=str(payload.get("claimed_at", "")).strip(),
                expires_at=str(payload.get("expires_at", "")).strip(),
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
