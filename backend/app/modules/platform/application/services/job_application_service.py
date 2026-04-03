from __future__ import annotations

from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import JobEventRepository, JobRepository
from app.modules.platform.infra.persistence.models import JobRecord


class JobApplicationService:
    def __init__(self, job_repository: JobRepository, job_event_repository: JobEventRepository) -> None:
        self.job_repository = job_repository
        self.job_event_repository = job_event_repository

    def create_job(self, user_id: int, command: CreateJobCommand) -> JobRecord:
        job = self.job_repository.create_job_with_items(user_id=user_id, command=command)
        self.job_event_repository.append(
            job_id=job.id,
            user_id=user_id,
            event_type="job.created",
            payload={"status": job.status.value, "job_type": job.job_type},
        )
        return job

    def start_job(self, user_id: int, command: StartJobCommand) -> JobRecord | None:
        job = self.job_repository.find_by_id_for_user(command.job_id, user_id)
        if job is None:
            return None
        for item in job.items:
            if item.status == JobItemStatus.PENDING:
                item.status = JobItemStatus.READY
        job.status = JobStatus.QUEUED
        self.job_repository.save(job)
        self.job_event_repository.append(
            job_id=job.id,
            user_id=user_id,
            event_type="job.queued",
            payload={"status": job.status.value, "triggered_by": command.triggered_by},
        )
        return job

    def cancel_job(self, user_id: int, command: CancelJobCommand) -> bool:
        job = self.job_repository.find_by_id_for_user(command.job_id, user_id)
        if job is None:
            return False
        changed = self.job_repository.mark_cancel_requested(job.id, user_id, job.updated_at)
        if changed:
            self.job_event_repository.append(
                job_id=job.id,
                user_id=user_id,
                event_type="job.cancel_requested",
                payload={"reason": command.reason},
            )
        return changed
