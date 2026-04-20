from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_routing_service import ExecutionRoutingService
from app.modules.platform.application.services.server_workspace_lock_service import (
    ServerWorkspaceBusyError,
)
from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    CredentialHealthCheckRecord,
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
from app.modules.platform.infra.persistence.repositories.execution_routing_repository_sqlalchemy import (
    ExecutionRoutingRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_charge_repository_sqlalchemy import (
    ExecutionChargeRepositorySqlAlchemy,
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
from app.modules.platform.infra.persistence.repositories.server_credential_admin_repository_sqlalchemy import (
    ServerCredentialAdminRepositorySqlAlchemy,
)
from app.modules.platform.runner.workflow_registry import PlatformWorkflowStep


def _seed_ready_job(db_session):
    job_repository = JobRepositorySqlAlchemy(db_session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.03.31", "items": [{"item_type": "card"}]}
        ),
    )
    job.status = JobStatus.QUEUED
    job.items[0].status = JobItemStatus.READY
    db_session.flush()
    return job


def _seed_ready_job_with_server_profile(db_session):
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
            credential_ciphertext="cipher-primary",
            secret_ciphertext=None,
            base_url="https://api.openai.com/v1",
            label="primary",
            priority=5,
            enabled=True,
            health_status="healthy",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )
    )
    job_repository = JobRepositorySqlAlchemy(db_session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [{"item_type": "card"}],
            }
        ),
    )
    job.status = JobStatus.QUEUED
    job.items[0].status = JobItemStatus.READY
    db_session.flush()
    return job, profile


def _seed_ready_job_with_two_server_credentials(db_session, cipher: ServerCredentialCipher):
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
    credential_a = ServerCredentialRecord(
        execution_profile_id=profile.id,
        provider="openai",
        auth_type="api_key",
        credential_ciphertext=cipher.encrypt("sk-primary"),
        secret_ciphertext=None,
        base_url="https://api-a.example.com/v1",
        label="primary",
        priority=5,
        enabled=True,
        health_status="healthy",
        last_checked_at=None,
        last_error_code="",
        last_error_message="",
    )
    credential_b = ServerCredentialRecord(
        execution_profile_id=profile.id,
        provider="openai",
        auth_type="api_key",
        credential_ciphertext=cipher.encrypt("sk-secondary"),
        secret_ciphertext=None,
        base_url="https://api-b.example.com/v1",
        label="secondary",
        priority=10,
        enabled=True,
        health_status="healthy",
        last_checked_at=None,
        last_error_code="",
        last_error_message="",
    )
    db_session.add_all([credential_a, credential_b])
    db_session.flush()
    job = JobRepositorySqlAlchemy(db_session).create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [{"item_type": "card"}],
            }
        ),
    )
    job.status = JobStatus.RUNNING
    job.items[0].status = JobItemStatus.RUNNING
    db_session.flush()
    execution = AIExecutionRepositorySqlAlchemy(db_session).create(
        AIExecutionRecord(
            job_id=job.id,
            job_item_id=job.items[0].id,
            user_id=1001,
            status="running",
            provider="openai",
            model="gpt-5.4",
            credential_ref=f"server-credential:{credential_a.id}",
            retry_attempt=0,
            switched_credential=False,
            request_idempotency_key="idem-retry",
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="text.generate",
            step_id="text.generate",
        )
    )
    db_session.flush()
    return job, profile, credential_a, credential_b, execution


class _SingleStepRegistry:
    def resolve(self, job_type: str, item_type: str, input_payload: dict[str, object] | None = None):
        return [PlatformWorkflowStep(step_type="text.generate", step_id="text.generate")]


class _BusyWorkspaceRegistry:
    def resolve(self, job_type: str, item_type: str, input_payload: dict[str, object] | None = None):
        return [PlatformWorkflowStep(step_type="code.generate", step_id="code.generate")]


class _BusyWorkspaceLockService:
    def acquire_write_lock(self, **kwargs):
        raise ServerWorkspaceBusyError(
            "server workspace is busy",
            server_project_ref=str(kwargs.get("server_project_ref", "")),
        )

    def release_write_lock(self, **kwargs):
        raise AssertionError("release_write_lock should not be called when acquire fails")


class _RetryOnceRunner:
    def __init__(self, *, first_error: str, success_text: str = "done") -> None:
        self.first_error = first_error
        self.success_text = success_text
        self.binding_refs: list[str] = []

    async def run(self, *, steps, base_request):
        self.binding_refs.append(base_request.execution_binding.credential_ref)
        if len(self.binding_refs) == 1:
            return [
                StepExecutionResult(
                    step_id=steps[-1].step_id,
                    status="failed_system",
                    error_summary=self.first_error,
                )
            ]
        return [
            StepExecutionResult(
                step_id=steps[-1].step_id,
                status="succeeded",
                output_payload={"text": self.success_text},
            )
        ]


def test_execution_orchestrator_service_creates_execution_when_quota_is_available(db_session):
    job = _seed_ready_job(db_session)
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    now = datetime.now(UTC)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    execution = service.start_execution(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        provider="openai",
        model="gpt-5.4",
        credential_ref="cred-a",
        retry_attempt=1,
        switched_credential=True,
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-1",
        request_idempotency_key="idem-run",
        now=now,
    )
    db_session.commit()

    assert execution is not None
    assert execution.job_id == job.id
    assert execution.credential_ref == "cred-a"
    assert execution.retry_attempt == 1
    assert execution.switched_credential is True


def test_execution_orchestrator_service_resolves_execution_route_from_selected_profile(db_session):
    job, profile = _seed_ready_job_with_server_profile(db_session)
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    now = datetime.now(UTC)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
    )

    execution = service.start_execution(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-route",
        request_idempotency_key="idem-route",
        now=now,
    )
    db_session.commit()

    assert execution is not None
    assert execution.provider == "openai"
    assert execution.model == "gpt-5.4"
    assert execution.credential_ref == "server-credential:1"
    assert execution.retry_attempt == 0
    assert execution.switched_credential is False
    assert execution.job_id == job.id
    assert profile.id == job.selected_execution_profile_id


def test_execution_orchestrator_service_raises_when_selected_profile_has_no_routable_credential(db_session):
    job, profile = _seed_ready_job_with_server_profile(db_session)
    credential = (
        db_session.query(ServerCredentialRecord)
        .filter(ServerCredentialRecord.execution_profile_id == profile.id)
        .one()
    )
    credential.health_status = "rate_limited"
    db_session.flush()
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=QuotaAccountRepositorySqlAlchemy(db_session),
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
    )

    try:
        service.start_execution(
            user_id=1001,
            job_id=job.id,
            job_item_id=job.items[0].id,
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="image.generate",
            step_id="step-no-route",
            request_idempotency_key="idem-no-route",
            now=datetime.now(UTC),
        )
    except LookupError as error:
        assert str(error) == f"no enabled healthy server credential for execution profile: {profile.id}"
    else:
        raise AssertionError("expected LookupError when no routable credential exists")


def test_execution_orchestrator_service_marks_quota_exhausted_when_reserve_fails(db_session):
    job = _seed_ready_job(db_session)
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=QuotaAccountRepositorySqlAlchemy(db_session),
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    execution = service.start_execution(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        provider="openai",
        model="gpt-5.4",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-2",
        request_idempotency_key="idem-no-quota",
        now=datetime.now(UTC),
    )
    db_session.commit()

    assert execution is None
    assert job.status == JobStatus.QUOTA_EXHAUSTED
    assert job.items[0].status == JobItemStatus.QUOTA_SKIPPED


def test_execution_orchestrator_service_persists_artifacts_from_succeeded_result(db_session):
    job = _seed_ready_job(db_session)
    job.status = JobStatus.RUNNING
    job.items[0].status = JobItemStatus.RUNNING
    execution = AIExecutionRepositorySqlAlchemy(db_session).create(
        AIExecutionRecord(
            job_id=job.id,
            job_item_id=job.items[0].id,
            user_id=1001,
            status=AIExecutionStatus.RUNNING,
            provider="openai",
            model="gpt-5.4",
            credential_ref="server-credential:1",
            retry_attempt=0,
            switched_credential=False,
            request_idempotency_key="idem-artifacts",
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="build.project",
            step_id="build.project",
        )
    )
    db_session.flush()

    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=None,
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
    )

    service._apply_result(
        job=job,
        item=job.items[0],
        execution=execution,
        result=StepExecutionResult(
            step_id="build.project",
            status="succeeded",
            output_payload={
                "text": "已完成 SingleEffectPatch 的服务器项目构建",
                "artifacts": [
                    {
                        "artifact_type": "build_output",
                        "storage_provider": "server_workspace",
                        "object_key": "/runtime/SingleEffectPatch.dll",
                        "file_name": "SingleEffectPatch.dll",
                        "mime_type": "application/octet-stream",
                        "size_bytes": 3,
                        "result_summary": "服务器构建产物",
                    }
                ],
            },
        ),
        now=datetime.now(UTC),
    )

    artifacts = ArtifactRepositorySqlAlchemy(db_session).list_by_execution(execution.id)
    assert artifacts[0].file_name == "SingleEffectPatch.dll"
    assert artifacts[0].artifact_type == "build_output"


def test_execution_orchestrator_service_retries_with_alternate_credential_on_retryable_failure(db_session):
    cipher = ServerCredentialCipher("retry-test-secret")
    job, profile, credential_a, credential_b, execution = _seed_ready_job_with_two_server_credentials(db_session, cipher)
    runner = _RetryOnceRunner(first_error="401 invalid token", success_text="retry succeeded")
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=None,
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        workflow_registry=_SingleStepRegistry(),
        workflow_runner=runner,
        server_credential_admin_repository=ServerCredentialAdminRepositorySqlAlchemy(db_session),
    )

    result = service.run_registered_workflow_to_completion(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        job_type="single_generate",
        item_type="card",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        input_payload={"item_name": "DarkBlade", "description": "测试"},
        now=datetime.now(UTC),
    )
    db_session.refresh(execution)

    assert result is not None
    assert result.status == "succeeded"
    assert runner.binding_refs == [
        f"server-credential:{credential_a.id}",
        f"server-credential:{credential_b.id}",
    ]
    assert execution.credential_ref == f"server-credential:{credential_b.id}"
    assert execution.retry_attempt == 1
    assert execution.switched_credential is True
    assert execution.result_summary == "retry succeeded"
    db_session.refresh(credential_a)
    assert credential_a.health_status == "degraded"
    health_check = (
        db_session.query(CredentialHealthCheckRecord)
        .filter(CredentialHealthCheckRecord.server_credential_id == credential_a.id)
        .one()
    )
    assert health_check.trigger_source == "runtime_retry"
    assert health_check.status == "degraded"
    assert health_check.error_code == "http_401"
    assert profile.id == job.selected_execution_profile_id


def test_execution_orchestrator_service_keeps_original_failure_when_no_alternate_credential_exists(db_session):
    cipher = ServerCredentialCipher("retry-test-secret")
    job, _, credential_a, credential_b, execution = _seed_ready_job_with_two_server_credentials(db_session, cipher)
    credential_b.health_status = "rate_limited"
    db_session.flush()
    runner = _RetryOnceRunner(first_error="401 invalid token")
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=None,
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        workflow_registry=_SingleStepRegistry(),
        workflow_runner=runner,
    )

    result = service.run_registered_workflow_to_completion(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        job_type="single_generate",
        item_type="card",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        input_payload={"item_name": "DarkBlade", "description": "测试"},
        now=datetime.now(UTC),
    )
    db_session.refresh(execution)

    assert result is not None
    assert result.status == "failed_system"
    assert runner.binding_refs == [f"server-credential:{credential_a.id}"]
    assert execution.credential_ref == f"server-credential:{credential_a.id}"
    assert execution.retry_attempt == 0
    assert execution.switched_credential is False


def test_execution_orchestrator_service_rejects_busy_server_workspace_before_running_write_steps(db_session):
    job, _ = _seed_ready_job_with_server_profile(db_session)
    job.items[0].input_payload = {
        "item_name": "DarkBlade",
        "description": "测试",
        "server_project_ref": "server-workspace:busy-token",
    }
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    now = datetime.now(UTC)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )
    db_session.flush()

    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        workflow_registry=_BusyWorkspaceRegistry(),
        workflow_runner=None,
        server_workspace_lock_service=_BusyWorkspaceLockService(),
    )

    try:
        service.start_execution(
            user_id=1001,
            job_id=job.id,
            job_item_id=job.items[0].id,
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="workflow.dispatch",
            step_id="workflow.dispatch",
            request_idempotency_key="idem-busy-workspace",
            now=datetime.now(UTC),
        )
    except ServerWorkspaceBusyError as error:
        assert str(error) == "server workspace is busy"
    else:
        raise AssertionError("expected ServerWorkspaceBusyError when workspace lock cannot be acquired")

    db_session.refresh(job)
    assert job.status == JobStatus.QUEUED
    assert job.items[0].status == JobItemStatus.READY
    assert db_session.query(AIExecutionRecord).count() == 0
