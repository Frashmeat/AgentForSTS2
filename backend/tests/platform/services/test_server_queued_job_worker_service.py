from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

from sqlalchemy.orm import sessionmaker

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.execution_routing_service import ExecutionRoutingService
from app.modules.platform.application.services.job_application_service import JobApplicationService
from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.application.services.server_deploy_target_lock_service import ServerDeployTargetBusyError
from app.modules.platform.application.services.server_queued_job_worker_service import ServerQueuedJobWorkerService
from app.modules.platform.application.services.server_workspace_lock_service import ServerWorkspaceBusyError
from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.models import (
    ExecutionProfileRecord,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBucketRecord,
    QuotaBucketType,
    ServerCredentialRecord,
)
from app.modules.platform.infra.persistence.repositories.ai_execution_repository_sqlalchemy import (
    AIExecutionRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.artifact_repository_sqlalchemy import (
    ArtifactRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_charge_repository_sqlalchemy import (
    ExecutionChargeRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_routing_repository_sqlalchemy import (
    ExecutionRoutingRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_event_repository_sqlalchemy import (
    JobEventRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.modules.platform.infra.persistence.repositories.quota_account_repository_sqlalchemy import (
    QuotaAccountRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.usage_ledger_repository_sqlalchemy import (
    UsageLedgerRepositorySqlAlchemy,
)
from app.modules.platform.runner.workflow_registry import PlatformWorkflowStep


class _SupportedWorkerRegistry:
    def resolve(self, job_type: str, item_type: str, input_payload: dict[str, object] | None = None):
        if (job_type, item_type) == ("single_generate", "card"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="single-card-plan",
                    input_payload={"asset_type": "card"},
                )
            ]
        if (job_type, item_type) == ("single_generate", "custom_code"):
            return [
                PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="single-custom-code-plan"),
                PlatformWorkflowStep(step_type="code.generate", step_id="single-custom-codegen"),
                PlatformWorkflowStep(step_type="build.project", step_id="single-custom-code-build"),
            ]
        raise KeyError(f"workflow not found for {job_type}/{item_type}")


class _SucceededWorkerRunner:
    async def run(self, *, steps, base_request):
        results: list[StepExecutionResult] = []
        payload = dict(base_request.input_payload)
        for step in steps:
            merged = dict(payload)
            merged.update(step.input_payload)
            if step.step_type == "single.asset.plan":
                output_payload = {"text": "已生成服务器卡牌实现方案"}
            elif step.step_type == "batch.custom_code.plan":
                output_payload = {
                    "text": "已生成服务器 custom_code 实现方案",
                    "item_name": str(merged.get("item_name", "")).strip(),
                    "server_workspace_root": str(merged.get("server_workspace_root", "")).strip(),
                }
            elif step.step_type == "code.generate":
                output_payload = {"text": f"已写入 {str(merged.get('item_name', '')).strip()} 的服务器 custom_code 代码"}
            else:
                item_name = str(merged.get("item_name", "")).strip()
                project_name = str(merged.get("server_project_name", "")).strip()
                output_payload = {
                    "text": f"已完成 {item_name} 的服务器项目构建",
                    "deployed_to": f"/Mods/{project_name}" if project_name else "",
                }
            results.append(
                StepExecutionResult(
                    step_id=step.step_id,
                    status="succeeded",
                    output_payload=output_payload,
                )
            )
            payload.update(output_payload)
        return results


class _DeployBusyWorkerRunner:
    async def run(self, *, steps, base_request):
        results: list[StepExecutionResult] = []
        payload = dict(base_request.input_payload)
        for step in steps:
            merged = dict(payload)
            merged.update(step.input_payload)
            if step.step_type == "batch.custom_code.plan":
                output_payload = {
                    "text": "已生成服务器 custom_code 实现方案",
                    "item_name": str(merged.get("item_name", "")).strip(),
                    "server_workspace_root": str(merged.get("server_workspace_root", "")).strip(),
                }
                results.append(
                    StepExecutionResult(
                        step_id=step.step_id,
                        status="succeeded",
                        output_payload=output_payload,
                    )
                )
                payload.update(output_payload)
                continue
            if step.step_type == "code.generate":
                output_payload = {"text": f"已写入 {str(merged.get('item_name', '')).strip()} 的服务器 custom_code 代码"}
                results.append(
                    StepExecutionResult(
                        step_id=step.step_id,
                        status="succeeded",
                        output_payload=output_payload,
                    )
                )
                payload.update(output_payload)
                continue
            project_name = str(merged.get("server_project_name", "")).strip()
            results.append(
                StepExecutionResult(
                    step_id=step.step_id,
                    status="failed_system",
                    error_summary="server deploy target is busy",
                    error_payload={
                        "reason_code": ServerDeployTargetBusyError.reason_code,
                        "reason_message": "server deploy target is busy",
                        "resource_type": ServerDeployTargetBusyError.resource_type,
                        "resource_key": project_name,
                        "project_name": project_name,
                    },
                )
            )
            break
        return results


class _BusyWorkspaceLockService:
    def acquire_write_lock(self, **kwargs):
        raise ServerWorkspaceBusyError(
            "server workspace is busy",
            server_project_ref=str(kwargs.get("server_project_ref", "")),
        )

    def release_write_lock(self, handle):
        raise AssertionError("release_write_lock should not be called when acquire fails")


def _seed_profile_and_quota(db_session, *, user_ids: list[int], secret: str) -> tuple[ExecutionProfileRecord, ServerCredentialCipher]:
    cipher = ServerCredentialCipher(secret)
    profile = ExecutionProfileRecord(
        code="codex-gpt-5-4",
        display_name="Codex CLI / gpt-5.4",
        agent_backend="codex",
        model="gpt-5.4",
        description="默认推荐",
        enabled=True,
        recommended=True,
        sort_order=10,
    )
    db_session.add(profile)
    db_session.flush()
    db_session.add(
        ServerCredentialRecord(
            execution_profile_id=profile.id,
            provider="openai",
            auth_type="api_key",
            credential_ciphertext=cipher.encrypt("sk-live-openai"),
            secret_ciphertext=None,
            base_url="https://api.openai.com/v1",
            label="main",
            priority=1,
            enabled=True,
            health_status="healthy",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )
    )
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    for user_id in user_ids:
        account = quota_repository.create_account(QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE))
        quota_repository.create_bucket(
            QuotaBucketRecord(
                quota_account_id=account.id,
                bucket_type=QuotaBucketType.DAILY,
                period_start=datetime.now(UTC) - timedelta(hours=1),
                period_end=datetime.now(UTC) + timedelta(hours=23),
                quota_limit=10,
                used_amount=0,
                refunded_amount=0,
            )
        )
    db_session.commit()
    return profile, cipher


def _build_job_service_builder(
    db_session,
    *,
    workflow_runner,
    workflow_registry,
    cipher: ServerCredentialCipher,
    server_workspace_service: ServerWorkspaceService | None = None,
    server_workspace_lock_service=None,
):
    session_factory = sessionmaker(bind=db_session.bind, autoflush=False, autocommit=False, expire_on_commit=False)

    def builder(session):
        return JobApplicationService(
            job_repository=JobRepositorySqlAlchemy(session),
            job_event_repository=JobEventRepositorySqlAlchemy(session),
            execution_orchestrator_service=ExecutionOrchestratorService(
                job_repository=JobRepositorySqlAlchemy(session),
                ai_execution_repository=AIExecutionRepositorySqlAlchemy(session),
                artifact_repository=ArtifactRepositorySqlAlchemy(session),
                quota_billing_service=QuotaBillingService(
                    execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(session),
                    quota_account_repository=QuotaAccountRepositorySqlAlchemy(session),
                    usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(session),
                ),
                job_event_repository=JobEventRepositorySqlAlchemy(session),
                execution_routing_service=ExecutionRoutingService(
                    execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(session)
                ),
                server_credential_cipher=cipher,
                server_workspace_lock_service=server_workspace_lock_service,
                server_workspace_service=server_workspace_service,
                workflow_registry=workflow_registry,
                workflow_runner=workflow_runner,
            ),
            server_workspace_service=server_workspace_service,
        )

    return session_factory, builder


def _create_queued_job(
    db_session,
    *,
    user_id: int,
    profile_id: int,
    job_type: str,
    item_type: str,
    input_payload: dict[str, object],
    queued_at: datetime,
):
    repository = JobRepositorySqlAlchemy(db_session)
    event_repository = JobEventRepositorySqlAlchemy(db_session)
    job = repository.create_job_with_items(
        user_id=user_id,
        command=CreateJobCommand.model_validate(
            {
                "job_type": job_type,
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile_id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": item_type,
                        "input_payload": input_payload,
                    }
                ],
            }
        ),
    )
    job.status = JobStatus.QUEUED
    job.started_at = queued_at
    job.items[0].status = JobItemStatus.READY
    event = event_repository.append(
        job_id=job.id,
        user_id=user_id,
        event_type="job.queued",
        payload={"status": JobStatus.QUEUED.value, "triggered_by": "seed"},
        job_item_id=job.items[0].id,
    )
    event.created_at = queued_at
    db_session.commit()
    return job


def test_server_queued_job_worker_service_can_resume_runnable_queued_job(db_session):
    profile, cipher = _seed_profile_and_quota(db_session, user_ids=[1001], secret="worker-test-secret")
    session_factory, builder = _build_job_service_builder(
        db_session,
        workflow_runner=_SucceededWorkerRunner(),
        workflow_registry=_SupportedWorkerRegistry(),
        cipher=cipher,
    )
    job = _create_queued_job(
        db_session,
        user_id=1001,
        profile_id=profile.id,
        job_type="single_generate",
        item_type="card",
        input_payload={"item_name": "DarkBlade"},
        queued_at=datetime.now(UTC) - timedelta(seconds=30),
    )
    worker = ServerQueuedJobWorkerService(
        session_factory=session_factory,
        job_application_service_builder=builder,
        retry_cooldown_seconds=5,
    )

    result = worker.run_once()

    db_session.expire_all()
    reloaded = JobRepositorySqlAlchemy(db_session).find_by_id_for_user(job.id, 1001)
    assert result.attempted_job_id == job.id
    assert result.started is True
    assert reloaded is not None
    assert reloaded.status == JobStatus.SUCCEEDED
    assert reloaded.items[0].status == JobItemStatus.SUCCEEDED


def test_server_queued_job_worker_service_respects_retry_cooldown(db_session):
    profile, cipher = _seed_profile_and_quota(db_session, user_ids=[1001], secret="worker-test-secret")
    session_factory, builder = _build_job_service_builder(
        db_session,
        workflow_runner=_SucceededWorkerRunner(),
        workflow_registry=_SupportedWorkerRegistry(),
        cipher=cipher,
    )
    job = _create_queued_job(
        db_session,
        user_id=1001,
        profile_id=profile.id,
        job_type="single_generate",
        item_type="card",
        input_payload={"item_name": "DarkBlade"},
        queued_at=datetime.now(UTC) - timedelta(seconds=2),
    )
    worker = ServerQueuedJobWorkerService(
        session_factory=session_factory,
        job_application_service_builder=builder,
        retry_cooldown_seconds=10,
    )

    result = worker.run_once()

    db_session.expire_all()
    reloaded = JobRepositorySqlAlchemy(db_session).find_by_id_for_user(job.id, 1001)
    assert result.attempted_job_id is None
    assert result.started is False
    assert result.reason == "no_runnable_queued_job"
    assert reloaded is not None
    assert reloaded.status == JobStatus.QUEUED
    assert reloaded.items[0].status == JobItemStatus.READY


def test_server_queued_job_worker_service_keeps_workspace_busy_job_queued(db_session, tmp_path):
    profile, cipher = _seed_profile_and_quota(db_session, user_ids=[1001], secret="worker-test-secret")
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    session_factory, builder = _build_job_service_builder(
        db_session,
        workflow_runner=_SucceededWorkerRunner(),
        workflow_registry=_SupportedWorkerRegistry(),
        cipher=cipher,
        server_workspace_service=workspace_service,
        server_workspace_lock_service=_BusyWorkspaceLockService(),
    )
    job = _create_queued_job(
        db_session,
        user_id=1001,
        profile_id=profile.id,
        job_type="single_generate",
        item_type="custom_code",
        input_payload={
            "item_name": "QueuedPatch",
            "server_project_ref": workspace.server_project_ref,
            "server_project_name": "DarkMod",
        },
        queued_at=datetime.now(UTC) - timedelta(seconds=30),
    )
    worker = ServerQueuedJobWorkerService(
        session_factory=session_factory,
        job_application_service_builder=builder,
        retry_cooldown_seconds=0,
    )

    result = worker.run_once()

    db_session.expire_all()
    reloaded = JobRepositorySqlAlchemy(db_session).find_by_id_for_user(job.id, 1001)
    assert result.attempted_job_id == job.id
    assert result.started is True
    assert reloaded is not None
    assert reloaded.status == JobStatus.QUEUED
    assert reloaded.items[0].status == JobItemStatus.READY
    assert reloaded.error_summary == "server workspace is busy"


def test_server_queued_job_worker_service_keeps_deploy_busy_job_queued(db_session, tmp_path):
    profile, cipher = _seed_profile_and_quota(db_session, user_ids=[1001], secret="worker-test-secret")
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    session_factory, builder = _build_job_service_builder(
        db_session,
        workflow_runner=_DeployBusyWorkerRunner(),
        workflow_registry=_SupportedWorkerRegistry(),
        cipher=cipher,
        server_workspace_service=workspace_service,
    )
    job = _create_queued_job(
        db_session,
        user_id=1001,
        profile_id=profile.id,
        job_type="single_generate",
        item_type="custom_code",
        input_payload={
            "item_name": "QueuedPatch",
            "server_project_ref": workspace.server_project_ref,
            "server_project_name": "DarkMod",
        },
        queued_at=datetime.now(UTC) - timedelta(seconds=30),
    )
    worker = ServerQueuedJobWorkerService(
        session_factory=session_factory,
        job_application_service_builder=builder,
        retry_cooldown_seconds=0,
    )

    result = worker.run_once()

    db_session.expire_all()
    reloaded = JobRepositorySqlAlchemy(db_session).find_by_id_for_user(job.id, 1001)
    assert result.attempted_job_id == job.id
    assert result.started is True
    assert reloaded is not None
    assert reloaded.status == JobStatus.QUEUED
    assert reloaded.items[0].status == JobItemStatus.READY
    assert reloaded.error_summary == "server deploy target is busy"
