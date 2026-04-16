from __future__ import annotations

from datetime import UTC, datetime

from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import JobEventRepository, JobRepository
from app.modules.platform.infra.persistence.models import JobRecord

from .execution_orchestrator_service import ExecutionOrchestratorService


_STEP_PROTOCOL_VERSION = "v1"
_RESULT_SCHEMA_VERSION = "v1"
_DISPATCH_STEP_TYPE = "workflow.dispatch"


class JobApplicationService:
    def __init__(
        self,
        job_repository: JobRepository,
        job_event_repository: JobEventRepository,
        execution_orchestrator_service: ExecutionOrchestratorService | None = None,
    ) -> None:
        self.job_repository = job_repository
        self.job_event_repository = job_event_repository
        self.execution_orchestrator_service = execution_orchestrator_service

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
        now = datetime.now(UTC)
        for item in job.items:
            if item.status == JobItemStatus.PENDING:
                item.status = JobItemStatus.READY
        job.status = JobStatus.QUEUED
        if job.started_at is None:
            job.started_at = now
        self._sync_job_counters(job)
        self.job_repository.save(job)
        self.job_event_repository.append(
            job_id=job.id,
            user_id=user_id,
            event_type="job.queued",
            payload={"status": job.status.value, "triggered_by": command.triggered_by},
        )
        if self.execution_orchestrator_service is not None:
            ready_items = [item for item in job.items if item.status == JobItemStatus.READY]
            for item in ready_items:
                execution = self.execution_orchestrator_service.start_execution(
                    user_id=user_id,
                    job_id=job.id,
                    job_item_id=item.id,
                    workflow_version=job.workflow_version,
                    step_protocol_version=_STEP_PROTOCOL_VERSION,
                    result_schema_version=_RESULT_SCHEMA_VERSION,
                    step_type=_DISPATCH_STEP_TYPE,
                    step_id=self._build_dispatch_step_id(item.item_index),
                    request_idempotency_key=self._build_start_idempotency_key(job.id, item.id),
                    now=now,
                )
                if execution is None:
                    continue
                self.execution_orchestrator_service.run_registered_workflow_to_completion(
                    user_id=user_id,
                    job_id=job.id,
                    job_item_id=item.id,
                    job_type=job.job_type,
                    item_type=item.item_type,
                    workflow_version=job.workflow_version,
                    step_protocol_version=_STEP_PROTOCOL_VERSION,
                    result_schema_version=_RESULT_SCHEMA_VERSION,
                    input_payload=item.input_payload,
                    now=now,
                )
            self._sync_job_counters(job)
            self._sync_job_status(job)
            self.job_repository.save(job)
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

    @staticmethod
    def _build_dispatch_step_id(item_index: int) -> str:
        return f"{_DISPATCH_STEP_TYPE}.item-{item_index + 1}"

    @staticmethod
    def _build_start_idempotency_key(job_id: int, job_item_id: int) -> str:
        return f"job-start:{job_id}:item:{job_item_id}"

    @staticmethod
    def _sync_job_counters(job: JobRecord) -> None:
        items = list(job.items)
        job.total_item_count = len(items)
        job.pending_item_count = sum(1 for item in items if item.status in {JobItemStatus.PENDING, JobItemStatus.READY})
        job.running_item_count = sum(1 for item in items if item.status == JobItemStatus.RUNNING)
        job.succeeded_item_count = sum(1 for item in items if item.status == JobItemStatus.SUCCEEDED)
        job.failed_business_item_count = sum(1 for item in items if item.status == JobItemStatus.FAILED_BUSINESS)
        job.failed_system_item_count = sum(1 for item in items if item.status == JobItemStatus.FAILED_SYSTEM)
        job.quota_skipped_item_count = sum(1 for item in items if item.status == JobItemStatus.QUOTA_SKIPPED)
        job.cancelled_before_start_item_count = sum(
            1 for item in items if item.status == JobItemStatus.CANCELLED_BEFORE_START
        )
        job.cancelled_after_start_item_count = sum(
            1 for item in items if item.status == JobItemStatus.CANCELLED_AFTER_START
        )

    @staticmethod
    def _sync_job_status(job: JobRecord) -> None:
        if job.running_item_count > 0:
            job.status = JobStatus.RUNNING
            return
        if job.pending_item_count > 0:
            job.status = JobStatus.QUEUED
            return
        if job.total_item_count == 0:
            job.status = JobStatus.QUEUED
            return
        if job.succeeded_item_count == job.total_item_count:
            job.status = JobStatus.SUCCEEDED
            job.result_summary = JobApplicationService._pick_first_non_empty(item.result_summary for item in job.items)
            job.error_summary = ""
            return
        if job.total_item_count > 0 and job.quota_skipped_item_count == job.total_item_count:
            job.status = JobStatus.QUOTA_EXHAUSTED
            job.error_summary = JobApplicationService._pick_first_non_empty(item.error_summary for item in job.items)
            return
        if job.failed_business_item_count + job.failed_system_item_count + job.quota_skipped_item_count > 0:
            if job.succeeded_item_count > 0:
                job.status = JobStatus.PARTIAL_SUCCEEDED
                job.result_summary = JobApplicationService._pick_first_non_empty(item.result_summary for item in job.items)
            else:
                job.status = JobStatus.FAILED
                job.result_summary = ""
            job.error_summary = JobApplicationService._pick_first_non_empty(item.error_summary for item in job.items)
            return
        job.status = JobStatus.QUEUED

    @staticmethod
    def _pick_first_non_empty(values) -> str:
        for value in values:
            text = str(value).strip()
            if text:
                return text
        return ""
