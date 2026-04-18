from __future__ import annotations

import asyncio
from datetime import datetime

from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.contracts.runner_contracts import (
    StepExecutionBinding,
    StepExecutionRequest,
    StepExecutionResult,
)
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import AIExecutionRepository, JobEventRepository, JobRepository
from app.modules.platform.infra.persistence.models import AIExecutionRecord
from app.modules.platform.runner.workflow_registry import PlatformWorkflowRegistry
from app.modules.platform.runner.workflow_runner import WorkflowRunner

from .execution_routing_service import ExecutionRoutingService
from .server_credential_cipher import ServerCredentialCipher
from .server_workspace_service import ServerWorkspaceService
from .uploaded_asset_service import UploadedAssetService


class ExecutionOrchestratorService:
    def __init__(
        self,
        job_repository: JobRepository,
        ai_execution_repository: AIExecutionRepository,
        quota_billing_service: QuotaBillingService | None,
        job_event_repository: JobEventRepository,
        execution_routing_service: ExecutionRoutingService | None = None,
        server_credential_cipher: ServerCredentialCipher | None = None,
        server_workspace_service: ServerWorkspaceService | None = None,
        uploaded_asset_service: UploadedAssetService | None = None,
        workflow_registry: PlatformWorkflowRegistry | None = None,
        workflow_runner: WorkflowRunner | None = None,
    ) -> None:
        self.job_repository = job_repository
        self.ai_execution_repository = ai_execution_repository
        self.quota_billing_service = quota_billing_service
        self.job_event_repository = job_event_repository
        self.execution_routing_service = execution_routing_service
        self.server_credential_cipher = server_credential_cipher
        self.server_workspace_service = server_workspace_service
        self.uploaded_asset_service = uploaded_asset_service
        self.workflow_registry = workflow_registry
        self.workflow_runner = workflow_runner

    def start_execution(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        provider: str = "",
        model: str = "",
        credential_ref: str = "",
        retry_attempt: int = 0,
        switched_credential: bool = False,
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

        if self.execution_routing_service is not None and not provider.strip() and not model.strip():
            route = self.execution_routing_service.resolve_for_job(job)
            provider = route.provider
            model = route.model
            credential_ref = route.credential_ref
            retry_attempt = route.retry_attempt
            switched_credential = route.switched_credential
        elif not provider.strip() or not model.strip():
            raise ValueError("provider and model are required when execution routing service is not configured")

        if self.quota_billing_service is None:
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
                credential_ref=credential_ref,
                retry_attempt=retry_attempt,
                switched_credential=switched_credential,
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

    async def run_registered_steps(
        self,
        *,
        user_id: int | None = None,
        job_type: str,
        item_type: str,
        job_id: int,
        job_item_id: int,
        workflow_version: str,
        step_protocol_version: str,
        result_schema_version: str,
        input_payload: dict[str, object] | None = None,
        execution_binding: StepExecutionBinding | dict[str, object] | None = None,
    ) -> list[StepExecutionResult]:
        if self.workflow_registry is None or self.workflow_runner is None:
            raise RuntimeError("workflow runner is not configured")
        binding = self._resolve_step_execution_binding(
            user_id=user_id,
            job_id=job_id,
            job_item_id=job_item_id,
            execution_binding=execution_binding,
        )
        payload = self._hydrate_runtime_refs(user_id=user_id, input_payload=input_payload)
        steps = self.workflow_registry.resolve(job_type, item_type)
        return await self.workflow_runner.run(
            steps=steps,
            base_request=StepExecutionRequest(
                workflow_version=workflow_version,
                step_protocol_version=step_protocol_version,
                step_type="workflow.dispatch",
                step_id="workflow.dispatch",
                job_id=job_id,
                job_item_id=job_item_id,
                result_schema_version=result_schema_version,
                input_payload=payload,
                execution_binding=binding,
            ),
        )

    def _hydrate_runtime_refs(
        self,
        *,
        user_id: int | None,
        input_payload: dict[str, object] | None,
    ) -> dict[str, object]:
        payload = dict(input_payload or {})
        if user_id is None:
            if any(str(payload.get(key, "")).strip() for key in ("uploaded_asset_ref", "server_project_ref")):
                raise ValueError("user_id is required when runtime refs are provided")
            return payload

        uploaded_asset_ref = str(payload.get("uploaded_asset_ref", "")).strip()
        if uploaded_asset_ref:
            if self.uploaded_asset_service is None:
                raise RuntimeError("uploaded asset service is not configured")

            uploaded = self.uploaded_asset_service.get_asset(user_id=user_id, uploaded_asset_ref=uploaded_asset_ref)
            payload["uploaded_asset_file_name"] = uploaded.file_name
            payload["uploaded_asset_mime_type"] = uploaded.mime_type
            payload["uploaded_asset_size_bytes"] = uploaded.size_bytes

        server_project_ref = str(payload.get("server_project_ref", "")).strip()
        if server_project_ref:
            if self.server_workspace_service is None:
                raise RuntimeError("server workspace service is not configured")

            workspace = self.server_workspace_service.get_workspace(user_id=user_id, server_project_ref=server_project_ref)
            payload["server_project_name"] = workspace.project_name
            payload["server_workspace_root"] = workspace.workspace_root
        return payload

    def has_registered_workflow(self, *, job_type: str, item_type: str) -> bool:
        if self.workflow_registry is None or self.workflow_runner is None:
            return False
        try:
            self.workflow_registry.resolve(job_type, item_type)
        except KeyError:
            return False
        return True

    def run_registered_workflow_to_completion(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        job_type: str,
        item_type: str,
        workflow_version: str,
        step_protocol_version: str,
        result_schema_version: str,
        input_payload: dict[str, object] | None,
        now: datetime,
    ) -> StepExecutionResult | None:
        if not self.has_registered_workflow(job_type=job_type, item_type=item_type):
            return None

        job = self.job_repository.find_by_id_for_user(job_id, user_id)
        if job is None:
            raise LookupError(f"job not found for user: {job_id}")
        item = next((entry for entry in job.items if entry.id == job_item_id), None)
        if item is None:
            raise LookupError(f"job item not found: {job_item_id}")

        execution = self.ai_execution_repository.find_latest_by_job_item(job_item_id)
        if execution is None:
            raise LookupError(f"ai_execution not found for job_item: {job_item_id}")

        execution.status = AIExecutionStatus.RUNNING
        execution.input_payload = dict(input_payload or {})
        self.ai_execution_repository.save(execution)

        results = asyncio.run(
            self.run_registered_steps(
                user_id=user_id,
                job_type=job_type,
                item_type=item_type,
                job_id=job_id,
                job_item_id=job_item_id,
                workflow_version=workflow_version,
                step_protocol_version=step_protocol_version,
                result_schema_version=result_schema_version,
                input_payload=input_payload,
            )
        )
        final_result = results[-1] if results else StepExecutionResult(
            step_id=execution.step_id,
            status="failed_system",
            error_summary="workflow produced no result",
        )
        self._apply_result(
            job=job,
            item=item,
            execution=execution,
            result=final_result,
            now=now,
        )
        self.ai_execution_repository.save(execution)
        self.job_repository.save(job)
        self.job_event_repository.append(
            job_id=job.id,
            user_id=user_id,
            event_type="ai_execution.finished",
            payload={"execution_id": execution.id, "status": execution.status.value},
            job_item_id=job_item_id,
            ai_execution_id=execution.id,
        )
        return final_result

    def refund_deferred_execution(self, *, execution_id: int, now: datetime) -> None:
        if self.quota_billing_service is None:
            return
        self.quota_billing_service.refund(execution_id, now, reason="execution_deferred")

    def complete_deferred_execution(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        execution_id: int,
        reason_message: str,
        now: datetime,
    ) -> None:
        execution = self.ai_execution_repository.find_by_id_for_update(execution_id)
        if execution is None:
            raise LookupError(f"ai_execution not found: {execution_id}")
        execution.status = AIExecutionStatus.COMPLETED_WITH_REFUND
        execution.error_summary = reason_message
        execution.finished_at = now
        self.ai_execution_repository.save(execution)
        self.job_event_repository.append(
            job_id=job_id,
            user_id=user_id,
            event_type="ai_execution.finished",
            payload={"execution_id": execution.id, "status": execution.status.value},
            job_item_id=job_item_id,
            ai_execution_id=execution.id,
        )

    def _resolve_step_execution_binding(
        self,
        *,
        user_id: int | None,
        job_id: int,
        job_item_id: int,
        execution_binding: StepExecutionBinding | dict[str, object] | None,
    ) -> StepExecutionBinding:
        if isinstance(execution_binding, dict):
            return StepExecutionBinding.model_validate(execution_binding)
        if execution_binding is not None:
            return execution_binding
        if user_id is None:
            raise ValueError("user_id is required when execution_binding is not provided")
        if self.execution_routing_service is None:
            raise RuntimeError("execution routing service is not configured")
        if self.server_credential_cipher is None:
            raise RuntimeError("server credential cipher is not configured")

        job = self.job_repository.find_by_id_for_user(job_id, user_id)
        if job is None:
            raise LookupError(f"job not found for user: {job_id}")

        route = self.execution_routing_service.resolve_for_job(job)
        latest_execution = self.ai_execution_repository.find_latest_by_job_item(job_item_id)
        if latest_execution is not None:
            expected = (route.provider, route.model, route.credential_ref)
            actual = (latest_execution.provider, latest_execution.model, latest_execution.credential_ref)
            if actual != expected:
                raise ValueError(
                    "latest ai_execution route does not match execution routing result"
                )

        return StepExecutionBinding(
            agent_backend=route.agent_backend,
            provider=route.provider,
            model=route.model,
            credential_ref=route.credential_ref,
            auth_type=route.auth_type,
            credential=self.server_credential_cipher.decrypt(route.credential_ciphertext),
            secret=self.server_credential_cipher.decrypt(route.secret_ciphertext) if route.secret_ciphertext else "",
            base_url=route.base_url,
            retry_attempt=latest_execution.retry_attempt if latest_execution is not None else route.retry_attempt,
            switched_credential=(
                latest_execution.switched_credential if latest_execution is not None else route.switched_credential
            ),
        )

    def _apply_result(
        self,
        *,
        job,
        item,
        execution: AIExecutionRecord,
        result: StepExecutionResult,
        now: datetime,
    ) -> None:
        summary = self._pick_summary(result.output_payload, result.error_summary)
        if result.status == "succeeded":
            execution.status = AIExecutionStatus.SUCCEEDED
            execution.result_summary = summary
            execution.result_payload = dict(result.output_payload)
            execution.error_summary = ""
            execution.error_payload = {}
            item.status = JobItemStatus.SUCCEEDED
            item.result_summary = summary
            item.error_summary = ""
            if self.quota_billing_service is not None:
                self.quota_billing_service.capture(execution.id, now)
        else:
            execution.status = (
                AIExecutionStatus.FAILED_BUSINESS if result.status == "failed_business" else AIExecutionStatus.FAILED_SYSTEM
            )
            execution.result_summary = ""
            execution.result_payload = {}
            execution.error_summary = result.error_summary
            execution.error_payload = {"step_id": result.step_id}
            item.status = JobItemStatus.FAILED_BUSINESS if result.status == "failed_business" else JobItemStatus.FAILED_SYSTEM
            item.result_summary = ""
            item.error_summary = result.error_summary
            if self.quota_billing_service is not None:
                self.quota_billing_service.refund(execution.id, now, reason="execution_failed")

        item.attempt_count += 1
        if item.started_at is None:
            item.started_at = execution.started_at
        item.finished_at = now
        execution.finished_at = now
        job.finished_at = now

    @staticmethod
    def _pick_summary(payload: dict[str, object], fallback: str) -> str:
        for key in ("text", "summary", "message", "result_summary"):
            value = str(payload.get(key, "")).strip()
            if value:
                return value
        return fallback.strip()
