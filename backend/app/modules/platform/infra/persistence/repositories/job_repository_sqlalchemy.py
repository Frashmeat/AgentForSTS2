from __future__ import annotations

from datetime import datetime

from sqlalchemy.orm import Session, selectinload

from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import JobRepository
from app.modules.platform.infra.persistence.models import JobItemRecord, JobRecord


class JobRepositorySqlAlchemy(JobRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create_job_with_items(self, user_id: int, command: CreateJobCommand) -> JobRecord:
        items = [
            JobItemRecord(
                user_id=user_id,
                item_index=index,
                item_type=item.item_type,
                status=JobItemStatus.PENDING,
                input_summary=item.input_summary,
                input_payload=dict(item.input_payload),
            )
            for index, item in enumerate(command.items)
        ]
        job = JobRecord(
            user_id=user_id,
            job_type=command.job_type,
            status=JobStatus.DRAFT,
            workflow_version=command.workflow_version,
            input_summary=command.input_summary,
            selected_execution_profile_id=command.selected_execution_profile_id,
            selected_agent_backend=command.selected_agent_backend,
            selected_model=command.selected_model,
            total_item_count=len(items),
            pending_item_count=len(items),
            items=items,
        )
        self.session.add(job)
        self.session.flush()
        return job

    def find_by_id_for_user(self, job_id: int, user_id: int) -> JobRecord | None:
        return (
            self.session.query(JobRecord)
            .options(selectinload(JobRecord.items))
            .filter(JobRecord.id == job_id, JobRecord.user_id == user_id)
            .one_or_none()
        )

    def find_by_id_for_update(self, job_id: int, user_id: int) -> JobRecord | None:
        return self.find_by_id_for_user(job_id, user_id)

    def save(self, job: JobRecord) -> None:
        self.session.add(job)
        self.session.flush()

    def mark_cancel_requested(self, job_id: int, user_id: int, requested_at: datetime) -> bool:
        job = self.find_by_id_for_user(job_id, user_id)
        if job is None:
            return False
        job.status = JobStatus.CANCELLING
        job.cancel_requested_at = requested_at
        self.session.flush()
        return True

    def count_active_server_jobs_for_user(self, user_id: int, *, exclude_job_id: int | None = None) -> int:
        query = self.session.query(JobRecord).filter(
            JobRecord.user_id == user_id,
            JobRecord.selected_execution_profile_id.is_not(None),
            JobRecord.status.in_([JobStatus.QUEUED, JobStatus.RUNNING, JobStatus.CANCELLING]),
        )
        if exclude_job_id is not None:
            query = query.filter(JobRecord.id != exclude_job_id)
        return query.count()
