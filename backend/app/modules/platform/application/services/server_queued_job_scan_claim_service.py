from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

_CLAIM_VERSION = "v1"
_DEFAULT_SCAN_CLAIM_LEASE_SECONDS = 10
_DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS = 5


@dataclass(slots=True)
class ServerQueuedJobScanClaimHolder:
    claim_version: str
    leader_epoch: int
    owner_id: str
    owner_scope: str
    claimed_at: str
    renewed_at: str
    expires_at: str


@dataclass(slots=True)
class ServerQueuedJobScanClaimHandle:
    claim_path: Path
    owner_id: str
    holder: ServerQueuedJobScanClaimHolder


class ServerQueuedJobScanClaimBusyError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        current_holder: ServerQueuedJobScanClaimHolder | None = None,
    ) -> None:
        super().__init__(message)
        self.current_holder = current_holder


class ServerQueuedJobScanClaimService:
    def __init__(
        self,
        storage_root: Path | None = None,
        *,
        lease_seconds: int = _DEFAULT_SCAN_CLAIM_LEASE_SECONDS,
        recovery_claim_lease_seconds: int = _DEFAULT_RECOVERY_CLAIM_LEASE_SECONDS,
        claim_file_name: str = "queue-scan.claim.json",
    ) -> None:
        self.storage_root = (
            storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-queued-job-scan-claims"
        )
        self.lease_seconds = max(int(lease_seconds or 0), 1)
        self.recovery_claim_lease_seconds = max(int(recovery_claim_lease_seconds or 0), 1)
        self.claim_file_name = str(claim_file_name).strip() or "queue-scan.claim.json"

    def ensure_leadership(
        self,
        *,
        owner_id: str,
        owner_scope: str,
        current_handle: ServerQueuedJobScanClaimHandle | None = None,
    ) -> ServerQueuedJobScanClaimHandle:
        self.storage_root.mkdir(parents=True, exist_ok=True)
        claim_path = self.storage_root / self.claim_file_name
        recovering_path = self._recovering_path(claim_path)
        normalized_owner_id = str(owner_id).strip()
        normalized_owner_scope = str(owner_scope).strip() or "system_queue_worker"
        if not normalized_owner_id:
            raise ValueError("owner_id is required")

        while True:
            now = datetime.now(UTC)
            self._cleanup_stale_recovering_file(recovering_path, now)
            holder = self._build_holder(owner_id=normalized_owner_id, owner_scope=normalized_owner_scope, now=now)
            if current_handle is not None and current_handle.owner_id == normalized_owner_id:
                current_holder = self._read_holder(claim_path)
                if current_holder is not None and current_holder.owner_id == normalized_owner_id:
                    holder = self._build_holder(
                        owner_id=normalized_owner_id,
                        owner_scope=normalized_owner_scope,
                        now=now,
                        leader_epoch=current_holder.leader_epoch,
                        claimed_at=current_holder.claimed_at,
                    )
                    self._write_claim_file(claim_path, holder)
                    return ServerQueuedJobScanClaimHandle(
                        claim_path=claim_path,
                        owner_id=normalized_owner_id,
                        holder=holder,
                    )
            created = self._try_create_claim_file(claim_path, holder)
            if created:
                return ServerQueuedJobScanClaimHandle(
                    claim_path=claim_path,
                    owner_id=normalized_owner_id,
                    holder=holder,
                )

            current_holder = self._read_holder(claim_path)
            if current_holder is not None and not self._is_holder_expired(current_holder, now):
                raise ServerQueuedJobScanClaimBusyError(
                    "queued job scan claim is busy",
                    current_holder=current_holder,
                )

            recovering_created = self._try_create_recovering_file(recovering_path, now)
            if not recovering_created:
                refreshed_holder = self._read_holder(claim_path)
                raise ServerQueuedJobScanClaimBusyError(
                    "queued job scan claim is busy",
                    current_holder=refreshed_holder,
                )

            try:
                latest_holder = self._read_holder(claim_path)
                if latest_holder is not None and not self._is_holder_expired(latest_holder, now):
                    raise ServerQueuedJobScanClaimBusyError(
                        "queued job scan claim is busy",
                        current_holder=latest_holder,
                    )
                next_epoch = 1
                if latest_holder is not None:
                    next_epoch = max(int(latest_holder.leader_epoch or 0), 0) + 1
                holder = self._build_holder(
                    owner_id=normalized_owner_id,
                    owner_scope=normalized_owner_scope,
                    now=now,
                    leader_epoch=next_epoch,
                )
                claim_path.unlink(missing_ok=True)
                recovered = self._try_create_claim_file(claim_path, holder)
                if recovered:
                    return ServerQueuedJobScanClaimHandle(
                        claim_path=claim_path,
                        owner_id=normalized_owner_id,
                        holder=holder,
                    )
            finally:
                recovering_path.unlink(missing_ok=True)

    def release_leadership(self, handle: ServerQueuedJobScanClaimHandle) -> None:
        current_holder = self._read_holder(handle.claim_path)
        if current_holder is None:
            return
        if current_holder.owner_id != handle.owner_id:
            return
        handle.claim_path.unlink(missing_ok=True)

    def get_current_leader(self) -> ServerQueuedJobScanClaimHolder | None:
        claim_path = self.storage_root / self.claim_file_name
        holder = self._read_holder(claim_path)
        if holder is None:
            return None
        if self._is_holder_expired(holder, datetime.now(UTC)):
            return None
        return holder

    def get_failover_window_seconds(self) -> int:
        return self.lease_seconds

    def _build_holder(
        self,
        *,
        owner_id: str,
        owner_scope: str,
        now: datetime,
        leader_epoch: int = 1,
        claimed_at: str | None = None,
    ) -> ServerQueuedJobScanClaimHolder:
        claimed_at_value = str(claimed_at).strip() or now.isoformat()
        renewed_at = now.isoformat()
        expires_at = datetime.fromtimestamp(now.timestamp() + self.lease_seconds, tz=UTC).isoformat()
        return ServerQueuedJobScanClaimHolder(
            claim_version=_CLAIM_VERSION,
            leader_epoch=max(int(leader_epoch or 0), 1),
            owner_id=owner_id,
            owner_scope=owner_scope,
            claimed_at=claimed_at_value,
            renewed_at=renewed_at,
            expires_at=expires_at,
        )

    def _try_create_claim_file(self, claim_path: Path, holder: ServerQueuedJobScanClaimHolder) -> bool:
        try:
            fd = os.open(str(claim_path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(self._holder_payload(holder), stream, ensure_ascii=False, indent=2)
        except Exception:
            claim_path.unlink(missing_ok=True)
            raise
        return True

    def _write_claim_file(self, claim_path: Path, holder: ServerQueuedJobScanClaimHolder) -> None:
        temp_path = claim_path.with_suffix(f"{claim_path.suffix}.tmp")
        temp_path.write_text(
            json.dumps(self._holder_payload(holder), ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        temp_path.replace(claim_path)

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

    def _is_holder_expired(self, holder: ServerQueuedJobScanClaimHolder, now: datetime) -> bool:
        expires_at = self._parse_datetime(holder.expires_at)
        if expires_at is None:
            return True
        return expires_at <= now

    @staticmethod
    def _recovering_path(claim_path: Path) -> Path:
        return claim_path.with_suffix(".recovering.json")

    @staticmethod
    def _read_holder(claim_path: Path) -> ServerQueuedJobScanClaimHolder | None:
        try:
            payload = json.loads(claim_path.read_text(encoding="utf-8"))
        except Exception:
            return None
        try:
            return ServerQueuedJobScanClaimHolder(
                claim_version=str(payload.get("claim_version", "v1")).strip() or "v1",
                leader_epoch=int(payload.get("leader_epoch", 1) or 1),
                owner_id=str(payload.get("owner_id", "")).strip(),
                owner_scope=str(payload.get("owner_scope", "")).strip() or "system_queue_worker",
                claimed_at=str(payload.get("claimed_at", "")).strip(),
                renewed_at=str(payload.get("renewed_at", payload.get("claimed_at", ""))).strip(),
                expires_at=str(payload.get("expires_at", "")).strip(),
            )
        except Exception:
            return None

    @staticmethod
    def _holder_payload(holder: ServerQueuedJobScanClaimHolder) -> dict[str, object]:
        return {
            "claim_version": holder.claim_version,
            "leader_epoch": holder.leader_epoch,
            "owner_id": holder.owner_id,
            "owner_scope": holder.owner_scope,
            "claimed_at": holder.claimed_at,
            "renewed_at": holder.renewed_at,
            "expires_at": holder.expires_at,
        }

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
