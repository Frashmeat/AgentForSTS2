from __future__ import annotations

import asyncio
import logging
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Protocol

from app.modules.platform.contracts.job_commands import StartJobCommand

from .job_application_service import JobApplicationService


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


class ServerQueuedJobWorkerService:
    def __init__(
        self,
        *,
        session_factory: SessionFactory,
        job_application_service_builder: JobApplicationServiceBuilder,
        poll_interval_seconds: float = 3.0,
        retry_cooldown_seconds: int = 5,
        logger: logging.Logger | None = None,
    ) -> None:
        self.session_factory = session_factory
        self.job_application_service_builder = job_application_service_builder
        self.poll_interval_seconds = max(float(poll_interval_seconds), 0.5)
        self.retry_cooldown_seconds = max(int(retry_cooldown_seconds), 0)
        self.logger = logger or logging.getLogger(__name__)
        self._stop_event = asyncio.Event()
        self._task: asyncio.Task[None] | None = None

    async def start(self) -> None:
        if self._task is not None and not self._task.done():
            return
        self._stop_event = asyncio.Event()
        self._task = asyncio.create_task(self._run_loop(), name="platform-queued-job-worker")

    async def stop(self) -> None:
        task = self._task
        if task is None:
            return
        self._stop_event.set()
        try:
            await task
        finally:
            self._task = None

    async def _run_loop(self) -> None:
        while not self._stop_event.is_set():
            self.run_once()
            try:
                await asyncio.wait_for(self._stop_event.wait(), timeout=self.poll_interval_seconds)
            except asyncio.TimeoutError:
                continue

    def run_once(self) -> QueueWorkerTickResult:
        session = self.session_factory()
        try:
            service = self.job_application_service_builder(session)
            job = service.job_repository.find_next_runnable_queued_job(
                datetime.now(UTC),
                cooldown_seconds=self.retry_cooldown_seconds,
            )
            if job is None:
                session.rollback()
                return QueueWorkerTickResult(reason="no_runnable_queued_job")

            started = service.start_job(
                user_id=job.user_id,
                command=StartJobCommand(job_id=job.id, triggered_by="system_queue_worker"),
            )
            session.commit()
            return QueueWorkerTickResult(
                attempted_job_id=job.id,
                started=started is not None,
                reason="started" if started is not None else "job_not_found",
            )
        except Exception:
            session.rollback()
            self.logger.exception("server queued job worker tick failed")
            return QueueWorkerTickResult(reason="error")
        finally:
            session.close()
