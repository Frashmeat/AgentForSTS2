from __future__ import annotations

from datetime import UTC, datetime
from collections import deque

from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.domain.repositories import JobEventRepository, JobRepository
from app.modules.platform.infra.persistence.models import JobRecord

from .execution_orchestrator_service import ExecutionOrchestratorService, QueuedExecutionReason
from .server_queued_job_claim_service import ServerQueuedJobClaimBusyError, ServerQueuedJobClaimHandle, ServerQueuedJobClaimService
from .server_workspace_lock_service import ServerWorkspaceBusyError
from .server_workspace_service import ServerWorkspaceService
from .uploaded_asset_service import UploadedAssetService


_STEP_PROTOCOL_VERSION = "v1"
_RESULT_SCHEMA_VERSION = "v1"
_DISPATCH_STEP_TYPE = "workflow.dispatch"
_PLATFORM_SERVER_JOB_TYPES = {"single_generate", "batch_generate"}
_MAX_ACTIVE_SERVER_JOBS_PER_USER = 2
_QUEUED_REASON_SERVER_WORKSPACE_BUSY = "server_workspace_busy"
_QUEUED_REASON_SERVER_DEPLOY_TARGET_BUSY = "server_deploy_target_busy"
_FORBIDDEN_PLATFORM_PAYLOAD_FIELDS = {
    "name",
    "asset_name",
    "project_root",
    "has_uploaded_image",
    "provided_image_b64",
    "provided_image_name",
}


class JobApplicationService:
    def __init__(
        self,
        job_repository: JobRepository,
        job_event_repository: JobEventRepository,
        execution_orchestrator_service: ExecutionOrchestratorService | None = None,
        server_queued_job_claim_service: ServerQueuedJobClaimService | None = None,
        server_workspace_service: ServerWorkspaceService | None = None,
        uploaded_asset_service: UploadedAssetService | None = None,
    ) -> None:
        self.job_repository = job_repository
        self.job_event_repository = job_event_repository
        self.execution_orchestrator_service = execution_orchestrator_service
        self.server_queued_job_claim_service = server_queued_job_claim_service
        self.server_workspace_service = server_workspace_service
        self.uploaded_asset_service = uploaded_asset_service

    def create_job(self, user_id: int, command: CreateJobCommand) -> JobRecord:
        self._validate_platform_payloads(user_id=user_id, command=command)
        self._hydrate_server_workspace_payloads(user_id=user_id, command=command)
        job = self.job_repository.create_job_with_items(user_id=user_id, command=command)
        self.job_event_repository.append(
            job_id=job.id,
            user_id=user_id,
            event_type="job.created",
            payload={"status": job.status.value, "job_type": job.job_type},
        )
        return job

    def start_job(self, user_id: int, command: StartJobCommand) -> JobRecord | None:
        job, _, _, _ = self.start_job_attempt(
            user_id=user_id,
            command=command,
            should_drain=True,
        )
        return job

    def start_job_attempt(
        self,
        user_id: int,
        command: StartJobCommand,
        *,
        should_drain: bool = True,
    ) -> tuple[JobRecord | None, bool, list[str], list[str]]:
        handle: ServerQueuedJobClaimHandle | None = None
        if self.server_queued_job_claim_service is not None:
            try:
                handle = self.server_queued_job_claim_service.acquire_claim(
                    job_id=command.job_id,
                    owner_scope=f"job_start:{str(command.triggered_by or 'unknown').strip() or 'unknown'}",
                )
            except ServerQueuedJobClaimBusyError:
                job = self.job_repository.find_by_id_for_user(command.job_id, user_id)
                return job, False, [], []

        try:
            job, released_workspace_refs, released_deploy_targets = self._start_job_internal(
                user_id=user_id,
                command=command,
            )
        finally:
            if handle is not None and self.server_queued_job_claim_service is not None:
                self.server_queued_job_claim_service.release_claim(handle)

        if job is not None and should_drain:
            self._drain_queued_jobs_for_resources(
                released_workspace_refs=released_workspace_refs,
                released_deploy_targets=released_deploy_targets,
                attempted_job_ids={job.id},
            )
        return job, True, released_workspace_refs, released_deploy_targets

    def _start_job_internal(self, *, user_id: int, command: StartJobCommand) -> tuple[JobRecord | None, list[str], list[str]]:
        job = self.job_repository.find_by_id_for_user(command.job_id, user_id)
        if job is None:
            return None, [], []
        self._hydrate_server_workspace_payloads_for_job(user_id=user_id, job=job)
        if job.selected_execution_profile_id is not None:
            active_server_job_count = self.job_repository.count_active_server_jobs_for_user(
                user_id,
                exclude_job_id=job.id,
            )
            if active_server_job_count >= _MAX_ACTIVE_SERVER_JOBS_PER_USER:
                raise ValueError(
                    f"too many active server jobs for user: limit {_MAX_ACTIVE_SERVER_JOBS_PER_USER}"
                )
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
        released_workspace_refs: list[str] = []
        released_deploy_targets: list[str] = []
        if self.execution_orchestrator_service is not None:
            ready_items = [item for item in job.items if item.status == JobItemStatus.READY]
            for item in ready_items:
                try:
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
                except ServerWorkspaceBusyError as error:
                    item.status = JobItemStatus.READY
                    item.error_summary = str(error)
                    self.job_event_repository.append(
                        job_id=job.id,
                        user_id=user_id,
                        event_type="job.queued",
                        payload={
                            "status": JobStatus.QUEUED.value,
                            "triggered_by": command.triggered_by,
                            "reason_code": _QUEUED_REASON_SERVER_WORKSPACE_BUSY,
                            "reason_message": str(error),
                        },
                        job_item_id=item.id,
                    )
                    continue
                if execution is None:
                    continue
                has_registered_workflow = self.execution_orchestrator_service.has_registered_workflow(
                    job_type=job.job_type,
                    item_type=item.item_type,
                )
                if not has_registered_workflow:
                    deferred_payload = self._build_deferred_payload(
                        execution_id=execution.id,
                        job_type=job.job_type,
                        item_type=item.item_type,
                        input_payload=item.input_payload,
                    )
                    item.status = JobItemStatus.DEFERRED
                    item.error_summary = str(deferred_payload.get("reason_message", ""))
                    self.execution_orchestrator_service.refund_deferred_execution(
                        execution_id=execution.id,
                        now=now,
                    )
                    self.job_event_repository.append(
                        job_id=job.id,
                        user_id=user_id,
                        event_type="ai_execution.deferred",
                        payload=deferred_payload,
                        job_item_id=item.id,
                        ai_execution_id=execution.id,
                    )
                    self.execution_orchestrator_service.complete_deferred_execution(
                        user_id=user_id,
                        job_id=job.id,
                        job_item_id=item.id,
                        execution_id=execution.id,
                        reason_message=str(deferred_payload.get("reason_message", "")),
                        now=now,
                    )
                    continue
                try:
                    final_result = self.execution_orchestrator_service.run_registered_workflow_to_completion(
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
                except QueuedExecutionReason as reason:
                    item.status = JobItemStatus.READY
                    item.error_summary = reason.reason_message
                    self.job_event_repository.append(
                        job_id=job.id,
                        user_id=user_id,
                        event_type="job.queued",
                        payload={
                            "status": JobStatus.QUEUED.value,
                            "triggered_by": command.triggered_by,
                            "reason_code": reason.reason_code or _QUEUED_REASON_SERVER_DEPLOY_TARGET_BUSY,
                            "reason_message": reason.reason_message,
                            **dict(reason.payload),
                        },
                        job_item_id=item.id,
                    )
                    continue
                server_project_ref = str((item.input_payload or {}).get("server_project_ref", "")).strip()
                if server_project_ref:
                    released_workspace_refs.append(server_project_ref)
                project_name = str((item.input_payload or {}).get("server_project_name", "")).strip()
                deployed_to = ""
                if final_result is not None:
                    deployed_to = str((final_result.output_payload or {}).get("deployed_to", "")).strip()
                if project_name and deployed_to:
                    released_deploy_targets.append(project_name)
            self._sync_job_counters(job)
            self._sync_job_status(job)
            self.job_repository.save(job)
        return job, released_workspace_refs, released_deploy_targets

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
        deferred_item_count = sum(1 for item in job.items if item.status == JobItemStatus.DEFERRED)
        if job.running_item_count > 0:
            job.status = JobStatus.RUNNING
            return
        if job.pending_item_count > 0:
            job.status = JobStatus.QUEUED
            job.result_summary = ""
            job.error_summary = JobApplicationService._pick_first_non_empty(item.error_summary for item in job.items)
            return
        if deferred_item_count > 0:
            job.status = JobStatus.DEFERRED
            job.result_summary = JobApplicationService._pick_first_non_empty(item.result_summary for item in job.items)
            job.error_summary = JobApplicationService._pick_first_non_empty(item.error_summary for item in job.items)
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
        job.result_summary = ""
        job.error_summary = JobApplicationService._pick_first_non_empty(item.error_summary for item in job.items)

    @staticmethod
    def _pick_first_non_empty(values) -> str:
        for value in values:
            text = str(value).strip()
            if text:
                return text
        return ""

    def _validate_platform_payloads(self, *, user_id: int, command: CreateJobCommand) -> None:
        if command.job_type not in _PLATFORM_SERVER_JOB_TYPES:
            return

        for item in command.items:
            forbidden_fields = sorted(
                field_name for field_name in _FORBIDDEN_PLATFORM_PAYLOAD_FIELDS if field_name in item.input_payload
            )
            if forbidden_fields:
                fields_text = ", ".join(forbidden_fields)
                raise ValueError(
                    f"platform job payload for {command.job_type}/{item.item_type} contains forbidden fields: {fields_text}"
                )

            item_name = str(item.input_payload.get("item_name", "")).strip()
            if not item_name:
                raise ValueError(f"platform job payload for {command.job_type}/{item.item_type} requires item_name")

            uploaded_asset_ref = str(item.input_payload.get("uploaded_asset_ref", "")).strip()
            if uploaded_asset_ref:
                if self.uploaded_asset_service is None:
                    raise ValueError("uploaded asset service is not configured")
                self.uploaded_asset_service.ensure_accessible(user_id=user_id, uploaded_asset_ref=uploaded_asset_ref)

            server_project_ref = str(item.input_payload.get("server_project_ref", "")).strip()
            if command.job_type in {"single_generate", "batch_generate"} and item.item_type == "custom_code" and not server_project_ref:
                raise ValueError(
                    f"platform job payload for {command.job_type}/custom_code requires server_project_ref"
                )
            if (
                command.job_type in {"single_generate", "batch_generate"}
                and item.item_type == "card_fullscreen"
                and uploaded_asset_ref
                and not server_project_ref
            ):
                raise ValueError(
                    f"platform job payload for {command.job_type}/card_fullscreen requires server_project_ref when uploaded_asset_ref is present"
                )
            if server_project_ref:
                if self.server_workspace_service is None:
                    raise ValueError("server workspace service is not configured")
                self.server_workspace_service.ensure_accessible(user_id=user_id, server_project_ref=server_project_ref)

    def _hydrate_server_workspace_payloads(self, *, user_id: int, command: CreateJobCommand) -> None:
        if self.server_workspace_service is None:
            return
        for item in command.items:
            self._hydrate_server_workspace_payload(
                user_id=user_id,
                payload=item.input_payload,
            )

    def _hydrate_server_workspace_payloads_for_job(self, *, user_id: int, job: JobRecord) -> None:
        if self.server_workspace_service is None:
            return
        changed = False
        for item in job.items:
            changed = self._hydrate_server_workspace_payload(user_id=user_id, payload=item.input_payload) or changed
        if changed:
            self.job_repository.save(job)

    def _hydrate_server_workspace_payload(self, *, user_id: int, payload: dict[str, object]) -> bool:
        server_project_ref = str(payload.get("server_project_ref", "")).strip()
        if not server_project_ref or self.server_workspace_service is None:
            return False
        workspace = self.server_workspace_service.get_workspace(user_id=user_id, server_project_ref=server_project_ref)
        project_name = workspace.project_name
        if str(payload.get("server_project_name", "")).strip() == project_name:
            return False
        payload["server_project_name"] = project_name
        return True

    @staticmethod
    def _build_deferred_payload(
        *,
        execution_id: int,
        job_type: str,
        item_type: str,
        input_payload: dict[str, object],
    ) -> dict[str, object]:
        reason_code, reason_message = JobApplicationService._resolve_deferred_reason(
            job_type=job_type,
            item_type=item_type,
            input_payload=input_payload,
        )
        return {
            "execution_id": execution_id,
            "job_type": job_type,
            "item_type": item_type,
            "reason_code": reason_code,
            "reason_message": reason_message,
        }

    @staticmethod
    def _resolve_deferred_reason(
        *,
        job_type: str,
        item_type: str,
        input_payload: dict[str, object],
    ) -> tuple[str, str]:
        project_root = str(input_payload.get("project_root", "")).strip()
        if project_root:
            return (
                "local_project_root_required",
                "input_payload.project_root 指向用户本机目录，当前服务器运行时还不能直接消费。",
            )

        return (
            "workflow_not_registered",
            f"当前 web runtime 尚未为 {job_type}/{item_type} 注册可直接执行的服务器 workflow。",
        )

    def _drain_queued_jobs_for_resources(
        self,
        *,
        released_workspace_refs: list[str],
        released_deploy_targets: list[str],
        attempted_job_ids: set[int] | None = None,
    ) -> None:
        if not released_workspace_refs and not released_deploy_targets:
            return

        pending_workspace_refs = deque(
            ref for ref in dict.fromkeys(str(ref).strip() for ref in released_workspace_refs) if ref
        )
        pending_deploy_targets = deque(
            target for target in dict.fromkeys(str(target).strip() for target in released_deploy_targets) if target
        )
        attempted = set(attempted_job_ids or set())

        while pending_workspace_refs or pending_deploy_targets:
            if pending_workspace_refs:
                next_job = self.job_repository.find_next_queued_job_for_server_workspace(
                    pending_workspace_refs.popleft(),
                    exclude_job_ids=attempted,
                )
            else:
                next_job = self.job_repository.find_next_queued_job_for_server_deploy_target(
                    pending_deploy_targets.popleft(),
                    exclude_job_ids=attempted,
                )
            if next_job is None:
                continue

            attempted.add(next_job.id)
            resumed_job, claimed, resumed_workspace_refs, resumed_deploy_targets = self.start_job_attempt(
                user_id=next_job.user_id,
                command=StartJobCommand(
                    job_id=next_job.id,
                    triggered_by="system_workspace_resume",
                ),
                should_drain=False,
            )
            if resumed_job is None or not claimed:
                continue
            for released_ref in resumed_workspace_refs:
                normalized_ref = str(released_ref).strip()
                if normalized_ref:
                    pending_workspace_refs.append(normalized_ref)
            for project_name in resumed_deploy_targets:
                normalized_project_name = str(project_name).strip()
                if normalized_project_name:
                    pending_deploy_targets.append(normalized_project_name)
