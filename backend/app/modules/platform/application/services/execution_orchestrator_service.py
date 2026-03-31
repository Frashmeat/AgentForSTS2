from __future__ import annotations

from datetime import datetime

from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import AIExecutionRepository, JobEventRepository, JobRepository
from app.modules.platform.infra.persistence.models import AIExecutionRecord


class ExecutionOrchestratorService:
    def __init__(
        self,
        job_repository: JobRepository,
        ai_execution_repository: AIExecutionRepository,
        quota_billing_service: QuotaBillingService,
        job_event_repository: JobEventRepository,
    ) -> None:
        self.job_repository = job_repository
        self.ai_execution_repository = ai_execution_repository
        self.quota_billing_service = quota_billing_service
        self.job_event_repository = job_event_repository

    def start_execution(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        provider: str,
        model: str,
        workflow_version: str,
        step_protocol_version: str,
        result_schema_version: str,
        step_type: str,
        step_id: str,
        request_idempotency_key: str | None,
        now: datetime,
    ) -> AIExecutionRecord | None:
        if request_idempotency_key:
            existing = self.ai_execution_repository.find_by_scoped_idempotency(
                user_id=user_id,
                job_item_id=job_item_id,
                request_idempotency_key=request_idempotency_key,
            )
            if existing is not None:
                return existing

        job = self.job_repository.find_by_id_for_user(job_id, user_id)
        if job is None:
            return None
        item = next((entry for entry in job.items if entry.id == job_item_id), None)
        if item is None:
            return None

        if not self.quota_billing_service.has_available_quota(user_id=user_id, now=now, amount=1):
            item.status = JobItemStatus.QUOTA_SKIPPED
            job.status = JobStatus.QUOTA_EXHAUSTED
            self.job_repository.save(job)
            self.job_event_repository.append(
                job_id=job.id,
                user_id=user_id,
                event_type="job.partial_blocked_by_quota",
                payload={"job_item_id": job_item_id},
                job_item_id=job_item_id,
            )
            return None

        execution = self.ai_execution_repository.create(
            AIExecutionRecord(
                job_id=job.id,
                job_item_id=job_item_id,
                user_id=user_id,
                status=AIExecutionStatus.CREATED,
                provider=provider,
                model=model,
                request_idempotency_key=request_idempotency_key,
                workflow_version=workflow_version,
                step_protocol_version=step_protocol_version,
                result_schema_version=result_schema_version,
                step_type=step_type,
                step_id=step_id,
                started_at=now,
            )
        )
        reserved = self.quota_billing_service.reserve(user_id=user_id, execution_id=execution.id, now=now, amount=1)
        if reserved is None:
            item.status = JobItemStatus.QUOTA_SKIPPED
            job.status = JobStatus.QUOTA_EXHAUSTED
            self.job_repository.save(job)
            return None

        execution.status = AIExecutionStatus.DISPATCHING
        item.status = JobItemStatus.RUNNING
        job.status = JobStatus.RUNNING
        self.ai_execution_repository.save(execution)
        self.job_repository.save(job)
        self.job_event_repository.append(
            job_id=job.id,
            user_id=user_id,
            event_type="ai_execution.started",
            payload={"execution_id": execution.id, "status": execution.status.value},
            job_item_id=job_item_id,
            ai_execution_id=execution.id,
        )
        return execution
