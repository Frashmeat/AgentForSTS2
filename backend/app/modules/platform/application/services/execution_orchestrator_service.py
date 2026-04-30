from __future__ import annotations

import asyncio
import concurrent.futures
import logging
import traceback
from datetime import datetime

from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.contracts.runner_contracts import (
    StepExecutionBinding,
    StepExecutionRequest,
    StepExecutionResult,
)
from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionEvent,
)
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import (
    AIExecutionRepository,
    ArtifactRepository,
    JobEventRepository,
    JobRepository,
    ServerCredentialAdminRepository,
)
from app.modules.platform.infra.persistence.models import AIExecutionRecord, ArtifactRecord
from app.modules.platform.runner.workflow_registry import PlatformWorkflowRegistry
from app.modules.platform.runner.workflow_runner import WorkflowRunner

from .execution_routing_service import ExecutionRoutingService
from .server_credential_cipher import ServerCredentialCipher
from .server_deploy_target_lock_service import ServerDeployTargetBusyError
from .server_workspace_lock_service import ServerWorkspaceLockHandle, ServerWorkspaceLockService
from .server_workspace_service import ServerWorkspaceService
from .uploaded_asset_service import UploadedAssetService

logger = logging.getLogger(__name__)


def _short_text(value: object, limit: int = 300) -> str:
    text = str(value or "").replace("\n", "\\n").strip()
    if len(text) <= limit:
        return text
    return f"{text[:limit]}..."


def _run_coroutine_blocking(coro):
    """从同步代码完整跑一段协程，不论调用栈是否已在事件循环里。

    - 没有运行中的 loop：直接 asyncio.run。
    - 已在 loop 中（例如被 FastAPI/WebSocket 处理器调用）：丢到独立线程跑新 loop，
      避免 "asyncio.run() cannot be called from a running event loop" 死锁。
    """
    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(coro)

    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        return pool.submit(asyncio.run, coro).result()


class QueuedExecutionReason(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        reason_code: str,
        reason_message: str,
        payload: dict[str, object] | None = None,
    ) -> None:
        super().__init__(message)
        self.reason_code = reason_code
        self.reason_message = reason_message
        self.payload = dict(payload or {})


class ExecutionOrchestratorService:
    def __init__(
        self,
        job_repository: JobRepository,
        ai_execution_repository: AIExecutionRepository,
        quota_billing_service: QuotaBillingService | None,
        job_event_repository: JobEventRepository,
        execution_routing_service: ExecutionRoutingService | None = None,
        server_credential_cipher: ServerCredentialCipher | None = None,
        server_workspace_lock_service: ServerWorkspaceLockService | None = None,
        server_workspace_service: ServerWorkspaceService | None = None,
        uploaded_asset_service: UploadedAssetService | None = None,
        workflow_registry: PlatformWorkflowRegistry | None = None,
        workflow_runner: WorkflowRunner | None = None,
        artifact_repository: ArtifactRepository | None = None,
        server_credential_admin_repository: ServerCredentialAdminRepository | None = None,
        workstation_execution_client=None,
    ) -> None:
        self.job_repository = job_repository
        self.ai_execution_repository = ai_execution_repository
        self.artifact_repository = artifact_repository
        self.quota_billing_service = quota_billing_service
        self.job_event_repository = job_event_repository
        self.execution_routing_service = execution_routing_service
        self.server_credential_cipher = server_credential_cipher
        self.server_workspace_lock_service = server_workspace_lock_service
        self.server_workspace_service = server_workspace_service
        self.uploaded_asset_service = uploaded_asset_service
        self.workflow_registry = workflow_registry
        self.workflow_runner = workflow_runner
        self.server_credential_admin_repository = server_credential_admin_repository
        self.workstation_execution_client = workstation_execution_client
        self._workspace_write_locks: dict[int, ServerWorkspaceLockHandle] = {}

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
        # 流水线：幂等命中 → 锁定 job/item → 解析路由 → 配额预检 → 创建+预占 → 标记派发
        existing = self._find_existing_idempotent_execution(
            user_id=user_id,
            job_id=job_id,
            job_item_id=job_item_id,
            request_idempotency_key=request_idempotency_key,
        )
        if existing is not None:
            return existing

        job, item = self._fetch_job_and_item(user_id=user_id, job_id=job_id, job_item_id=job_item_id)
        if job is None or item is None:
            return None
        self._acquire_workspace_write_lock_if_needed(job=job, item=item)

        provider, model, credential_ref, retry_attempt, switched_credential = self._resolve_execution_route(
            job=job,
            user_id=user_id,
            job_id=job_id,
            job_item_id=job_item_id,
            provider=provider,
            model=model,
            credential_ref=credential_ref,
            retry_attempt=retry_attempt,
            switched_credential=switched_credential,
        )

        if not self._ensure_quota_available_or_mark_skipped(
            user_id=user_id,
            job=job,
            item=item,
            job_item_id=job_item_id,
            now=now,
        ):
            return None

        execution = self._create_execution_and_reserve_quota(
            user_id=user_id,
            job=job,
            item=item,
            job_item_id=job_item_id,
            now=now,
            provider=provider,
            model=model,
            credential_ref=credential_ref,
            retry_attempt=retry_attempt,
            switched_credential=switched_credential,
            workflow_version=workflow_version,
            step_protocol_version=step_protocol_version,
            result_schema_version=result_schema_version,
            step_type=step_type,
            step_id=step_id,
            request_idempotency_key=request_idempotency_key,
        )
        if execution is None:
            return None

        self._mark_execution_dispatching(
            user_id=user_id,
            job=job,
            item=item,
            job_item_id=job_item_id,
            execution=execution,
            step_type=step_type,
            step_id=step_id,
        )
        return execution

    # ── start_execution 流水线分段 ──────────────────────────────────────────

    def _find_existing_idempotent_execution(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        request_idempotency_key: str | None,
    ) -> AIExecutionRecord | None:
        if not request_idempotency_key:
            return None
        existing = self.ai_execution_repository.find_by_scoped_idempotency(
            user_id=user_id,
            job_item_id=job_item_id,
            request_idempotency_key=request_idempotency_key,
        )
        if existing is None:
            return None
        logger.info(
            "platform execution idempotency hit user_id=%s job_id=%s job_item_id=%s execution_id=%s status=%s",
            user_id,
            job_id,
            job_item_id,
            existing.id,
            existing.status,
        )
        return existing

    def _fetch_job_and_item(self, *, user_id: int, job_id: int, job_item_id: int):
        job = self.job_repository.find_by_id_for_user(job_id, user_id)
        if job is None:
            logger.warning("platform execution start skipped job not found user_id=%s job_id=%s", user_id, job_id)
            return None, None
        item = next((entry for entry in job.items if entry.id == job_item_id), None)
        if item is None:
            logger.warning(
                "platform execution start skipped item not found user_id=%s job_id=%s job_item_id=%s",
                user_id,
                job_id,
                job_item_id,
            )
            return job, None
        return job, item

    def _resolve_execution_route(
        self,
        *,
        job,
        user_id: int,
        job_id: int,
        job_item_id: int,
        provider: str,
        model: str,
        credential_ref: str,
        retry_attempt: int,
        switched_credential: bool,
    ) -> tuple[str, str, str, int, bool]:
        if self.execution_routing_service is not None and not provider.strip() and not model.strip():
            route = self.execution_routing_service.resolve_for_job(job)
            provider = route.provider
            model = route.model
            credential_ref = route.credential_ref
            retry_attempt = route.retry_attempt
            switched_credential = route.switched_credential
            log_kind = "resolved"
        elif not provider.strip() or not model.strip():
            raise ValueError("provider and model are required when execution routing service is not configured")
        else:
            log_kind = "provided"

        logger.info(
            "platform execution route %s user_id=%s job_id=%s job_item_id=%s provider=%s model=%s "
            "credential_ref=%s retry_attempt=%s switched=%s",
            log_kind,
            user_id,
            job_id,
            job_item_id,
            provider,
            model,
            credential_ref,
            retry_attempt,
            switched_credential,
        )
        return provider, model, credential_ref, retry_attempt, switched_credential

    def _ensure_quota_available_or_mark_skipped(
        self,
        *,
        user_id: int,
        job,
        item,
        job_item_id: int,
        now: datetime,
    ) -> bool:
        if self.quota_billing_service is None:
            logger.warning(
                "platform execution start skipped quota billing unavailable user_id=%s job_id=%s job_item_id=%s",
                user_id,
                job.id,
                job_item_id,
            )
            return False
        if not self.quota_billing_service.has_available_quota(user_id=user_id, now=now, amount=1):
            logger.info(
                "platform execution start blocked by quota user_id=%s job_id=%s job_item_id=%s",
                user_id,
                job.id,
                job_item_id,
            )
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
            return False
        return True

    def _create_execution_and_reserve_quota(
        self,
        *,
        user_id: int,
        job,
        item,
        job_item_id: int,
        now: datetime,
        provider: str,
        model: str,
        credential_ref: str,
        retry_attempt: int,
        switched_credential: bool,
        workflow_version: str,
        step_protocol_version: str,
        result_schema_version: str,
        step_type: str,
        step_id: str,
        request_idempotency_key: str | None,
    ) -> AIExecutionRecord | None:
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
        reserved = self.quota_billing_service.reserve(
            user_id=user_id,
            execution_id=execution.id,
            now=now,
            amount=1,
        )
        if reserved is None:
            logger.info(
                "platform execution quota reserve failed user_id=%s job_id=%s job_item_id=%s execution_id=%s",
                user_id,
                job.id,
                job_item_id,
                execution.id,
            )
            item.status = JobItemStatus.QUOTA_SKIPPED
            job.status = JobStatus.QUOTA_EXHAUSTED
            self.job_repository.save(job)
            return None

        logger.info(
            "platform execution quota reserved user_id=%s job_id=%s job_item_id=%s execution_id=%s provider=%s "
            "model=%s credential_ref=%s",
            user_id,
            job.id,
            job_item_id,
            execution.id,
            provider,
            model,
            credential_ref,
        )
        return execution

    def _mark_execution_dispatching(
        self,
        *,
        user_id: int,
        job,
        item,
        job_item_id: int,
        execution: AIExecutionRecord,
        step_type: str,
        step_id: str,
    ) -> None:
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
        logger.info(
            "platform execution dispatching user_id=%s job_id=%s job_item_id=%s execution_id=%s step_type=%s step_id=%s",
            user_id,
            job.id,
            job_item_id,
            execution.id,
            step_type,
            step_id,
        )

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
        steps = self._resolve_workflow_steps(
            job_type=job_type,
            item_type=item_type,
            input_payload=payload,
        )
        logger.info(
            "platform workflow steps resolved job_id=%s job_item_id=%s job_type=%s item_type=%s steps=%s",
            job_id,
            job_item_id,
            job_type,
            item_type,
            [f"{step.step_type}:{step.step_id}" for step in steps],
        )
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
            payload["uploaded_asset_path"] = str(
                self.uploaded_asset_service.get_asset_content_path(
                    user_id=user_id,
                    uploaded_asset_ref=uploaded_asset_ref,
                )
            )

        server_project_ref = str(payload.get("server_project_ref", "")).strip()
        if server_project_ref:
            if self.server_workspace_service is None:
                raise RuntimeError("server workspace service is not configured")

            workspace = self.server_workspace_service.get_workspace(
                user_id=user_id, server_project_ref=server_project_ref
            )
            payload["server_project_name"] = workspace.project_name
            payload["server_workspace_root"] = workspace.workspace_root
            payload["runtime_user_id"] = user_id
        return payload

    def has_registered_workflow(self, *, job_type: str, item_type: str) -> bool:
        if self.workflow_registry is None or self.workflow_runner is None:
            return False
        try:
            self._resolve_workflow_steps(job_type=job_type, item_type=item_type, input_payload={})
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
        try:
            if not self.has_registered_workflow(job_type=job_type, item_type=item_type):
                logger.info(
                    "platform workflow skipped no registered workflow user_id=%s job_id=%s job_item_id=%s "
                    "job_type=%s item_type=%s",
                    user_id,
                    job_id,
                    job_item_id,
                    job_type,
                    item_type,
                )
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

            logger.info(
                "platform workflow start user_id=%s job_id=%s job_item_id=%s execution_id=%s job_type=%s "
                "item_type=%s workflow_version=%s",
                user_id,
                job_id,
                job_item_id,
                execution.id,
                job_type,
                item_type,
                workflow_version,
            )
            execution.status = AIExecutionStatus.RUNNING
            execution.input_payload = dict(input_payload or {})
            self.ai_execution_repository.save(execution)

            final_result = self._run_workflow_once(
                user_id=user_id,
                job_type=job_type,
                item_type=item_type,
                job_id=job_id,
                job_item_id=job_item_id,
                execution=execution,
                workflow_version=workflow_version,
                step_protocol_version=step_protocol_version,
                result_schema_version=result_schema_version,
                input_payload=input_payload,
            )
            retry_binding = self._build_retry_execution_binding(
                user_id=user_id,
                job_id=job_id,
                job_item_id=job_item_id,
                execution=execution,
                result=final_result,
                now=now,
            )
            if retry_binding is not None:
                previous_credential_ref = execution.credential_ref
                logger.info(
                    "platform workflow retry scheduled user_id=%s job_id=%s job_item_id=%s execution_id=%s "
                    "from_credential_ref=%s to_credential_ref=%s retry_attempt=%s error=%s",
                    user_id,
                    job.id,
                    job_item_id,
                    execution.id,
                    previous_credential_ref,
                    retry_binding.credential_ref,
                    retry_binding.retry_attempt,
                    _short_text(final_result.error_summary),
                )
                execution.provider = retry_binding.provider
                execution.model = retry_binding.model
                execution.credential_ref = retry_binding.credential_ref
                execution.retry_attempt = retry_binding.retry_attempt
                execution.switched_credential = retry_binding.switched_credential
                self.ai_execution_repository.save(execution)
                self.job_event_repository.append(
                    job_id=job.id,
                    user_id=user_id,
                    event_type="ai_execution.retry_scheduled",
                    payload={
                        "execution_id": execution.id,
                        "from_credential_ref": previous_credential_ref,
                        "to_credential_ref": retry_binding.credential_ref,
                        "retry_attempt": retry_binding.retry_attempt,
                        "error_summary": final_result.error_summary,
                    },
                    job_item_id=job_item_id,
                    ai_execution_id=execution.id,
                )
                final_result = self._run_workflow_once(
                    user_id=user_id,
                    job_type=job_type,
                    item_type=item_type,
                    job_id=job_id,
                    job_item_id=job_item_id,
                    execution=execution,
                    workflow_version=workflow_version,
                    step_protocol_version=step_protocol_version,
                    result_schema_version=result_schema_version,
                    input_payload=input_payload,
                    execution_binding=retry_binding,
                )
            queued_reason = self._build_queued_execution_reason(final_result)
            if queued_reason is not None:
                logger.info(
                    "platform workflow requeued user_id=%s job_id=%s job_item_id=%s execution_id=%s "
                    "reason_code=%s reason=%s",
                    user_id,
                    job.id,
                    job_item_id,
                    execution.id,
                    queued_reason.reason_code,
                    _short_text(queued_reason.reason_message),
                )
                self._requeue_execution_after_busy_result(
                    item=item,
                    execution=execution,
                    queued_reason=queued_reason,
                    now=now,
                )
                self.ai_execution_repository.save(execution)
                self.job_event_repository.append(
                    job_id=job.id,
                    user_id=user_id,
                    event_type="ai_execution.finished",
                    payload={
                        "execution_id": execution.id,
                        "status": execution.status.value,
                        "reason_code": queued_reason.reason_code,
                        "reason_message": queued_reason.reason_message,
                    },
                    job_item_id=job_item_id,
                    ai_execution_id=execution.id,
                )
                raise queued_reason
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
            logger.info(
                "platform workflow finished user_id=%s job_id=%s job_item_id=%s execution_id=%s "
                "execution_status=%s result_status=%s reason_code=%s error=%s",
                user_id,
                job.id,
                job_item_id,
                execution.id,
                execution.status,
                final_result.status,
                str(final_result.error_payload.get("reason_code", "")).strip(),
                _short_text(final_result.error_summary),
            )
            return final_result
        finally:
            self._release_workspace_write_lock(job_item_id)

    def _build_retry_execution_binding(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        execution: AIExecutionRecord,
        result: StepExecutionResult,
        now: datetime,
    ) -> StepExecutionBinding | None:
        if not self._should_retry_with_alternate_credential(execution=execution, result=result):
            return None
        self._record_retryable_credential_failure(
            credential_ref=execution.credential_ref,
            error_summary=result.error_summary,
            now=now,
        )
        if self.execution_routing_service is None or self.server_credential_cipher is None:
            return None

        job = self.job_repository.find_by_id_for_user(job_id, user_id)
        if job is None:
            raise LookupError(f"job not found for user: {job_id}")
        try:
            route = self.execution_routing_service.resolve_retry_for_job(
                job,
                failed_credential_ref=execution.credential_ref,
            )
        except (LookupError, ValueError):
            return None
        return StepExecutionBinding(
            agent_backend=route.agent_backend,
            provider=route.provider,
            model=route.model,
            credential_ref=route.credential_ref,
            auth_type=route.auth_type,
            credential=self.server_credential_cipher.decrypt(route.credential_ciphertext),
            secret=self.server_credential_cipher.decrypt(route.secret_ciphertext) if route.secret_ciphertext else "",
            base_url=route.base_url,
            retry_attempt=route.retry_attempt,
            switched_credential=route.switched_credential,
        )

    def _record_retryable_credential_failure(
        self,
        *,
        credential_ref: str,
        error_summary: str,
        now: datetime,
    ) -> None:
        if self.server_credential_admin_repository is None:
            return
        credential_id = self._credential_id_from_ref(credential_ref)
        status, error_code = self._map_retryable_failure_to_health(error_summary)
        self.server_credential_admin_repository.record_health_check_result(
            credential_id=credential_id,
            trigger_source="runtime_retry",
            status=status,
            error_code=error_code,
            error_message=str(error_summary).strip(),
            latency_ms=None,
            checked_at=now,
        )

    @staticmethod
    def _should_retry_with_alternate_credential(
        *,
        execution: AIExecutionRecord,
        result: StepExecutionResult,
    ) -> bool:
        if result.status != "failed_system":
            return False
        if execution.retry_attempt > 0 or execution.switched_credential:
            return False
        credential_ref = str(execution.credential_ref or "").strip()
        if not credential_ref.startswith("server-credential:"):
            return False
        error_text = str(result.error_summary or "").lower()
        if not error_text:
            return False
        retryable_markers = (
            "401",
            "403",
            "429",
            "quota",
            "rate limit",
            "rate_limited",
            "temporarily unavailable",
            "unavailable",
            "connecterror",
            "connection",
            "timeout",
            "getaddrinfo",
        )
        return any(marker in error_text for marker in retryable_markers)

    @staticmethod
    def _map_retryable_failure_to_health(error_summary: str) -> tuple[str, str]:
        text = str(error_summary or "").lower()
        if "429" in text or "rate limit" in text or "rate_limited" in text:
            return "rate_limited", "rate_limited"
        if "quota" in text:
            return "rate_limited", "quota_exhausted"
        if "401" in text:
            return "degraded", "http_401"
        if "403" in text:
            return "degraded", "http_403"
        if "timeout" in text:
            return "degraded", "timeout"
        if "getaddrinfo" in text or "connecterror" in text or "connection" in text:
            return "degraded", "connection_error"
        if "temporarily unavailable" in text or "unavailable" in text:
            return "degraded", "temporarily_unavailable"
        return "degraded", "runtime_retryable_failure"

    @staticmethod
    def _credential_id_from_ref(credential_ref: str) -> int:
        prefix = "server-credential:"
        value = str(credential_ref).strip()
        if not value.startswith(prefix):
            raise ValueError(f"credential_ref is invalid: {credential_ref}")
        raw_id = value[len(prefix) :].strip()
        if not raw_id.isdigit():
            raise ValueError(f"credential_ref is invalid: {credential_ref}")
        return int(raw_id)

    def refund_deferred_execution(self, *, execution_id: int, now: datetime) -> None:
        if self.quota_billing_service is None:
            logger.warning(
                "platform deferred execution refund skipped quota billing unavailable execution_id=%s", execution_id
            )
            return
        self.quota_billing_service.refund(execution_id, now, reason="execution_deferred")
        logger.info(
            "platform deferred execution refunded execution_id=%s reason=%s", execution_id, "execution_deferred"
        )

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
        logger.info(
            "platform deferred execution completed user_id=%s job_id=%s job_item_id=%s execution_id=%s reason=%s",
            user_id,
            job_id,
            job_item_id,
            execution.id,
            _short_text(reason_message),
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
            binding = StepExecutionBinding.model_validate(execution_binding)
            logger.info(
                "platform step binding provided job_id=%s job_item_id=%s provider=%s model=%s "
                "credential_ref=%s retry_attempt=%s switched=%s",
                job_id,
                job_item_id,
                binding.provider,
                binding.model,
                binding.credential_ref,
                binding.retry_attempt,
                binding.switched_credential,
            )
            return binding
        if execution_binding is not None:
            logger.info(
                "platform step binding provided job_id=%s job_item_id=%s provider=%s model=%s "
                "credential_ref=%s retry_attempt=%s switched=%s",
                job_id,
                job_item_id,
                execution_binding.provider,
                execution_binding.model,
                execution_binding.credential_ref,
                execution_binding.retry_attempt,
                execution_binding.switched_credential,
            )
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
                raise ValueError("latest ai_execution route does not match execution routing result")

        binding = StepExecutionBinding(
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
        logger.info(
            "platform step binding resolved user_id=%s job_id=%s job_item_id=%s provider=%s model=%s "
            "credential_ref=%s retry_attempt=%s switched=%s",
            user_id,
            job_id,
            job_item_id,
            binding.provider,
            binding.model,
            binding.credential_ref,
            binding.retry_attempt,
            binding.switched_credential,
        )
        return binding

    def _run_workflow_once(
        self,
        *,
        user_id: int,
        job_type: str,
        item_type: str,
        job_id: int,
        job_item_id: int,
        execution: AIExecutionRecord,
        workflow_version: str,
        step_protocol_version: str,
        result_schema_version: str,
        input_payload: dict[str, object] | None,
        execution_binding: StepExecutionBinding | dict[str, object] | None = None,
    ) -> StepExecutionResult:
        if self.workstation_execution_client is not None:
            try:
                binding = self._resolve_step_execution_binding(
                    user_id=user_id,
                    job_id=job_id,
                    job_item_id=job_item_id,
                    execution_binding=execution_binding,
                )
                payload = self._hydrate_runtime_refs(user_id=user_id, input_payload=input_payload)
                workstation_execution_id = f"ws-exec-{execution.id}"

                def record_workstation_events(events: list[WorkstationExecutionEvent]) -> None:
                    for event in events:
                        self.job_event_repository.append(
                            job_id=job_id,
                            user_id=user_id,
                            event_type=event.event_type,
                            payload={
                                **event.payload,
                                "workstation_execution_id": workstation_execution_id,
                                "sequence": event.sequence,
                                "occurred_at": event.occurred_at,
                            },
                            job_item_id=job_item_id,
                            ai_execution_id=execution.id,
                        )

                return self.workstation_execution_client.dispatch_and_poll(
                    WorkstationExecutionDispatchRequest(
                        execution_id=execution.id,
                        job_id=job_id,
                        job_item_id=job_item_id,
                        job_type=job_type,
                        item_type=item_type,
                        workflow_version=workflow_version,
                        step_protocol_version=step_protocol_version,
                        result_schema_version=result_schema_version,
                        input_payload=payload,
                        execution_binding=binding,
                    ),
                    on_events=record_workstation_events,
                )
            except asyncio.CancelledError:
                # 外层任务被取消时不应吞掉 —— 让上游协调者继续传播
                raise
            except Exception as exc:
                # 工作站派发失败属于系统级错误，但需要保留 traceback 才能排障
                tb_text = traceback.format_exc()
                logger.exception(
                    "platform workstation execution failed job_id=%s job_item_id=%s execution_id=%s error=%s",
                    job_id,
                    job_item_id,
                    execution.id,
                    _short_text(exc),
                )
                return StepExecutionResult(
                    step_id=execution.step_id,
                    status="failed_system",
                    error_summary=str(exc),
                    error_payload={
                        "reason_code": "workstation_dispatch_failed",
                        "exception_type": type(exc).__name__,
                        "traceback": tb_text,
                    },
                )

        results = _run_coroutine_blocking(
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
                execution_binding=execution_binding,
            )
        )
        return (
            results[-1]
            if results
            else StepExecutionResult(
                step_id=execution.step_id,
                status="failed_system",
                error_summary="workflow produced no result",
            )
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
            self._persist_artifacts(
                user_id=job.user_id,
                job_id=job.id,
                job_item_id=item.id,
                execution_id=execution.id,
                output_payload=result.output_payload,
            )
            if self.quota_billing_service is not None:
                self.quota_billing_service.capture(execution.id, now)
                logger.info(
                    "platform execution quota captured user_id=%s job_id=%s job_item_id=%s execution_id=%s",
                    job.user_id,
                    job.id,
                    item.id,
                    execution.id,
                )
        else:
            execution.status = (
                AIExecutionStatus.FAILED_BUSINESS
                if result.status == "failed_business"
                else AIExecutionStatus.FAILED_SYSTEM
            )
            execution.result_summary = ""
            execution.result_payload = {}
            execution.error_summary = result.error_summary
            execution.error_payload = {"step_id": result.step_id, **dict(result.error_payload or {})}
            item.status = (
                JobItemStatus.FAILED_BUSINESS if result.status == "failed_business" else JobItemStatus.FAILED_SYSTEM
            )
            item.result_summary = ""
            item.error_summary = result.error_summary
            if self.quota_billing_service is not None:
                self.quota_billing_service.refund(execution.id, now, reason="execution_failed")
                logger.info(
                    "platform execution quota refunded user_id=%s job_id=%s job_item_id=%s execution_id=%s "
                    "reason=%s result_status=%s reason_code=%s error=%s",
                    job.user_id,
                    job.id,
                    item.id,
                    execution.id,
                    "execution_failed",
                    result.status,
                    str(result.error_payload.get("reason_code", "")).strip(),
                    _short_text(result.error_summary),
                )

        item.attempt_count += 1
        if item.started_at is None:
            item.started_at = execution.started_at
        item.finished_at = now
        execution.finished_at = now
        job.finished_at = now

    def _persist_artifacts(
        self,
        *,
        user_id: int,
        job_id: int,
        job_item_id: int,
        execution_id: int,
        output_payload: dict[str, object],
    ) -> None:
        if self.artifact_repository is None:
            return
        raw_artifacts = output_payload.get("artifacts")
        if not isinstance(raw_artifacts, list):
            return

        existing = {
            (artifact.storage_provider, artifact.object_key)
            for artifact in self.artifact_repository.list_by_execution(execution_id)
        }
        for raw in raw_artifacts:
            if not isinstance(raw, dict):
                continue
            storage_provider = str(raw.get("storage_provider", "")).strip()
            object_key = str(raw.get("object_key", "")).strip()
            artifact_type = str(raw.get("artifact_type", "")).strip() or "build_output"
            if not storage_provider or not object_key:
                continue
            dedupe_key = (storage_provider, object_key)
            if dedupe_key in existing:
                continue
            self.artifact_repository.create(
                ArtifactRecord(
                    job_id=job_id,
                    job_item_id=job_item_id,
                    ai_execution_id=execution_id,
                    user_id=user_id,
                    artifact_type=artifact_type,
                    storage_provider=storage_provider,
                    object_key=object_key,
                    file_name=str(raw.get("file_name", "")).strip() or None,
                    mime_type=str(raw.get("mime_type", "")).strip() or None,
                    size_bytes=int(raw.get("size_bytes", 0) or 0) or None,
                    result_summary=str(raw.get("result_summary", "")).strip(),
                )
            )
            existing.add(dedupe_key)

    @staticmethod
    def _pick_summary(payload: dict[str, object], fallback: str) -> str:
        for key in ("text", "summary", "message", "result_summary"):
            value = str(payload.get(key, "")).strip()
            if value:
                return value
        return fallback.strip()

    @staticmethod
    def _build_queued_execution_reason(result: StepExecutionResult) -> QueuedExecutionReason | None:
        if result.status != "failed_system":
            return None
        payload = dict(result.error_payload or {})
        reason_code = str(payload.get("reason_code", "")).strip()
        reason_message = str(payload.get("reason_message", "")).strip() or result.error_summary
        if reason_code != ServerDeployTargetBusyError.reason_code:
            return None
        return QueuedExecutionReason(
            reason_message,
            reason_code=reason_code,
            reason_message=reason_message,
            payload=payload,
        )

    def _requeue_execution_after_busy_result(
        self,
        *,
        item,
        execution: AIExecutionRecord,
        queued_reason: QueuedExecutionReason,
        now: datetime,
    ) -> None:
        if self.quota_billing_service is not None:
            self.quota_billing_service.refund(execution.id, now, reason="execution_requeued")
            logger.info(
                "platform execution quota refunded job_item_id=%s execution_id=%s reason=%s reason_code=%s",
                item.id,
                execution.id,
                "execution_requeued",
                queued_reason.reason_code,
            )
        execution.status = AIExecutionStatus.COMPLETED_WITH_REFUND
        execution.result_summary = ""
        execution.result_payload = {}
        execution.error_summary = queued_reason.reason_message
        execution.error_payload = dict(queued_reason.payload)
        execution.finished_at = now
        execution.request_idempotency_key = None
        item.status = JobItemStatus.READY
        item.result_summary = ""
        item.error_summary = queued_reason.reason_message
        item.attempt_count += 1
        if item.started_at is None:
            item.started_at = execution.started_at
        item.finished_at = None

    def _acquire_workspace_write_lock_if_needed(self, *, job, item) -> None:
        if self.server_workspace_lock_service is None:
            return
        if item.id in self._workspace_write_locks:
            return
        input_payload = dict(item.input_payload or {})
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if not server_project_ref:
            return
        try:
            steps = self._resolve_workflow_steps(
                job_type=job.job_type,
                item_type=item.item_type,
                input_payload=input_payload,
            )
        except (KeyError, RuntimeError):
            return
        if not any(step.step_type in {"code.generate", "asset.generate", "build.project"} for step in steps):
            return
        self._workspace_write_locks[item.id] = self.server_workspace_lock_service.acquire_write_lock(
            server_project_ref=server_project_ref,
            job_id=job.id,
            job_item_id=item.id,
            user_id=job.user_id,
        )

    def _release_workspace_write_lock(self, job_item_id: int) -> None:
        handle = self._workspace_write_locks.pop(job_item_id, None)
        if handle is None or self.server_workspace_lock_service is None:
            return
        self.server_workspace_lock_service.release_write_lock(handle)

    def _resolve_workflow_steps(
        self,
        *,
        job_type: str,
        item_type: str,
        input_payload: dict[str, object],
    ):
        if self.workflow_registry is None:
            raise RuntimeError("workflow registry is not configured")
        try:
            return self.workflow_registry.resolve(job_type, item_type, input_payload=input_payload)
        except TypeError:
            return self.workflow_registry.resolve(job_type, item_type)
