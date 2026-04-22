from __future__ import annotations

import asyncio
import logging
import os
from collections import deque
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Protocol
from uuid import uuid4

from app.modules.platform.contracts.job_commands import StartJobCommand

from .job_application_service import JobApplicationService
from .platform_runtime_audit_service import PlatformRuntimeAuditService
from .server_queued_job_scan_claim_service import (
    ServerQueuedJobScanClaimBusyError,
    ServerQueuedJobScanClaimHandle,
    ServerQueuedJobScanClaimService,
)


class _Session(Protocol):
    def commit(self) -> None: ...

    def rollback(self) -> None: ...

    def close(self) -> None: ...


SessionFactory = Callable[[], _Session]
JobApplicationServiceBuilder = Callable[[_Session], JobApplicationService]


@dataclass(slots=True)
class QueueWorkerTickResult:
    attempted_job_id: int | None = None
    started: bool = False
    reason: str = ""


@dataclass(slots=True)
class QueueWorkerLeaderEvent:
    event_type: str
    occurred_at: str
    owner_id: str
    leader_epoch: int | None = None
    detail: str = ""


_MAX_RECENT_LEADER_EVENTS = 20


class ServerQueuedJobWorkerService:
    def __init__(
        self,
        *,
        session_factory: SessionFactory,
        job_application_service_builder: JobApplicationServiceBuilder,
        scan_claim_service: ServerQueuedJobScanClaimService | None = None,
        runtime_audit_service: PlatformRuntimeAuditService | None = None,
        poll_interval_seconds: float = 3.0,
        retry_cooldown_seconds: int = 5,
        leader_retry_grace_seconds: float = 1.0,
        logger: logging.Logger | None = None,
    ) -> None:
        self.session_factory = session_factory
        self.job_application_service_builder = job_application_service_builder
        self.scan_claim_service = scan_claim_service
        self.runtime_audit_service = runtime_audit_service
        self.poll_interval_seconds = max(float(poll_interval_seconds), 0.5)
        self.retry_cooldown_seconds = max(int(retry_cooldown_seconds), 0)
        self.leader_retry_grace_seconds = max(float(leader_retry_grace_seconds), 0.0)
        self.logger = logger or logging.getLogger(__name__)
        self._stop_event = asyncio.Event()
        self._task: asyncio.Task[None] | None = None
        self._leader_claim_handle: ServerQueuedJobScanClaimHandle | None = None
        self._leader_owner_id = f"queue-worker:{os.getpid()}:{uuid4().hex}"
        self._leader_owner_scope = "system_queue_worker"
        self._last_tick_at: str = ""
        self._last_tick_reason: str = ""
        self._last_leader_acquired_at: str = ""
        self._last_leader_lost_at: str = ""
        self._last_observed_leader_owner_id: str = ""
        self._next_leader_retry_not_before: str = ""
        self._recent_leader_events: deque[QueueWorkerLeaderEvent] = deque(maxlen=_MAX_RECENT_LEADER_EVENTS)

    async def start(self) -> None:
        if self._task is not None and not self._task.done():
            return
        self._stop_event = asyncio.Event()
        self._task = asyncio.create_task(self._run_loop(), name="platform-queued-job-worker")

    async def stop(self) -> None:
        task = self._task
        if task is None:
            self._release_leadership()
            return
        self._stop_event.set()
        try:
            await task
        finally:
            self._task = None
            self._release_leadership()

    async def _run_loop(self) -> None:
        while not self._stop_event.is_set():
            self.run_once()
            try:
                await asyncio.wait_for(self._stop_event.wait(), timeout=self.poll_interval_seconds)
            except asyncio.TimeoutError:
                continue

    def run_once(self) -> QueueWorkerTickResult:
        if self.scan_claim_service is not None:
            now = datetime.now(UTC)
            retry_not_before = self._parse_datetime(self._next_leader_retry_not_before)
            if retry_not_before is not None and now < retry_not_before:
                return self._remember_tick(QueueWorkerTickResult(reason="leader_retry_backoff"))
            try:
                previous_owner = self._leader_claim_handle.owner_id if self._leader_claim_handle is not None else ""
                self._leader_claim_handle = self.scan_claim_service.ensure_leadership(
                    owner_id=self._leader_owner_id,
                    owner_scope=self._leader_owner_scope,
                    current_handle=self._leader_claim_handle,
                )
                if previous_owner != self._leader_owner_id:
                    self._last_leader_acquired_at = datetime.now(UTC).isoformat()
                    holder = self._leader_claim_handle.holder
                    self._record_leader_event(
                        event_type="leader_acquired" if holder.leader_epoch == 1 else "leader_taken_over",
                        leader_epoch=holder.leader_epoch,
                        detail="queue worker became leader",
                    )
                self._next_leader_retry_not_before = ""
                self._last_observed_leader_owner_id = self._leader_owner_id
            except ServerQueuedJobScanClaimBusyError:
                observed = self.scan_claim_service.get_current_leader() if self.scan_claim_service is not None else None
                observed_owner = observed.owner_id if observed is not None else ""
                if self._leader_claim_handle is not None:
                    self._last_leader_lost_at = datetime.now(UTC).isoformat()
                    self._record_leader_event(
                        event_type="leader_lost",
                        leader_epoch=observed.leader_epoch if observed is not None else None,
                        detail="queue worker lost leadership",
                    )
                if observed_owner and observed_owner != self._last_observed_leader_owner_id:
                    self._record_leader_event(
                        event_type="leader_observed_other",
                        owner_id=observed_owner,
                        leader_epoch=observed.leader_epoch if observed is not None else None,
                        detail="another worker currently holds leader lease",
                    )
                if observed is not None:
                    retry_at = self._parse_datetime(observed.expires_at)
                    if retry_at is not None:
                        retry_at = retry_at + timedelta(seconds=self.leader_retry_grace_seconds)
                        self._next_leader_retry_not_before = retry_at.isoformat()
                        self._record_leader_event(
                            event_type="leader_waiting_for_failover",
                            owner_id=observed.owner_id,
                            leader_epoch=observed.leader_epoch,
                            detail=f"retry after {self._next_leader_retry_not_before}",
                        )
                self._last_observed_leader_owner_id = observed_owner
                self._leader_claim_handle = None
                return self._remember_tick(QueueWorkerTickResult(reason="not_leader"))
        session = self.session_factory()
        try:
            service = self.job_application_service_builder(session)
            job = service.job_repository.find_next_runnable_queued_job(
                datetime.now(UTC),
                cooldown_seconds=self.retry_cooldown_seconds,
            )
            if job is None:
                session.rollback()
                return self._remember_tick(QueueWorkerTickResult(reason="no_runnable_queued_job"))

            started_job, claimed, _, _ = service.start_job_attempt(
                user_id=job.user_id,
                command=StartJobCommand(job_id=job.id, triggered_by="system_queue_worker"),
            )
            session.commit()
            return self._remember_tick(
                QueueWorkerTickResult(
                    attempted_job_id=job.id,
                    started=claimed and started_job is not None,
                    reason=(
                        "started"
                        if claimed and started_job is not None
                        else "job_claimed_by_other_consumer" if not claimed else "job_not_found"
                    ),
                )
            )
        except Exception:
            session.rollback()
            self.logger.exception("server queued job worker tick failed")
            return self._remember_tick(QueueWorkerTickResult(reason="error"))
        finally:
            session.close()

    def _release_leadership(self) -> None:
        if self._leader_claim_handle is None or self.scan_claim_service is None:
            return
        holder = self._leader_claim_handle.holder
        self.scan_claim_service.release_leadership(self._leader_claim_handle)
        self._leader_claim_handle = None
        self._last_leader_lost_at = datetime.now(UTC).isoformat()
        self._last_observed_leader_owner_id = ""
        self._next_leader_retry_not_before = ""
        self._record_leader_event(
            event_type="leader_released",
            leader_epoch=holder.leader_epoch,
            detail="queue worker released leader lease",
        )

    def _remember_tick(self, result: QueueWorkerTickResult) -> QueueWorkerTickResult:
        self._last_tick_at = datetime.now(UTC).isoformat()
        self._last_tick_reason = result.reason
        return result

    def _record_leader_event(
        self,
        *,
        event_type: str,
        owner_id: str | None = None,
        leader_epoch: int | None = None,
        detail: str = "",
    ) -> None:
        event = QueueWorkerLeaderEvent(
            event_type=event_type,
            occurred_at=datetime.now(UTC).isoformat(),
            owner_id=str(owner_id).strip() or self._leader_owner_id,
            leader_epoch=leader_epoch,
            detail=str(detail).strip(),
        )
        self._recent_leader_events.append(event)
        self.logger.info(
            "platform queue worker leader event: %s owner=%s epoch=%s detail=%s",
            event.event_type,
            event.owner_id,
            event.leader_epoch,
            event.detail,
        )
        if self.runtime_audit_service is not None:
            self.runtime_audit_service.append_event(
                event_type=f"runtime.queue_worker.{event.event_type}",
                payload={
                    "owner_id": event.owner_id,
                    "owner_scope": self._leader_owner_scope,
                    "leader_epoch": event.leader_epoch,
                    "detail": event.detail,
                },
            )

    def get_runtime_status(self) -> dict[str, object]:
        current_leader = None
        if self.scan_claim_service is not None:
            holder = self.scan_claim_service.get_current_leader()
            if holder is not None:
                current_leader = {
                    "leader_epoch": holder.leader_epoch,
                    "owner_id": holder.owner_id,
                    "owner_scope": holder.owner_scope,
                    "claimed_at": holder.claimed_at,
                    "renewed_at": holder.renewed_at,
                    "expires_at": holder.expires_at,
                }
        return {
            "owner_id": self._leader_owner_id,
            "owner_scope": self._leader_owner_scope,
            "is_leader": (
                current_leader is not None and current_leader.get("owner_id") == self._leader_owner_id
            ),
            "leader_epoch": current_leader.get("leader_epoch") if current_leader is not None else None,
            "failover_window_seconds": (
                self.scan_claim_service.get_failover_window_seconds() if self.scan_claim_service is not None else None
            ),
            "leader_retry_grace_seconds": self.leader_retry_grace_seconds,
            "next_leader_retry_not_before": self._next_leader_retry_not_before,
            "last_tick_at": self._last_tick_at,
            "last_tick_reason": self._last_tick_reason,
            "last_leader_acquired_at": self._last_leader_acquired_at,
            "last_leader_lost_at": self._last_leader_lost_at,
            "current_leader": current_leader,
            "recent_leader_events": [
                {
                    "event_type": event.event_type,
                    "occurred_at": event.occurred_at,
                    "owner_id": event.owner_id,
                    "leader_epoch": event.leader_epoch,
                    "detail": event.detail,
                }
                for event in self._recent_leader_events
            ],
            "running": self._task is not None and not self._task.done(),
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
