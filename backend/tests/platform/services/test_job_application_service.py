from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.execution_routing_service import ExecutionRoutingService
from app.modules.platform.application.services.job_application_service import JobApplicationService
from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.application.services.server_queued_job_claim_service import ServerQueuedJobClaimService
from app.modules.platform.application.services.server_workspace_lock_service import (
    ServerWorkspaceBusyError,
)
from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService
from app.modules.platform.application.services.uploaded_asset_service import UploadedAssetService
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionProfileRecord,
    JobEventRecord,
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


def test_job_application_service_handles_create_start_and_cancel(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )
    create_command = CreateJobCommand.model_validate(
        {
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "items": [
                {"item_type": "card", "input_payload": {"item_name": "One"}},
                {"item_type": "card", "input_payload": {"item_name": "Two"}},
            ],
        }
    )

    job = service.create_job(user_id=1001, command=create_command)
    assert job.status == JobStatus.DRAFT

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))
    assert started is not None
    assert started.status == JobStatus.QUEUED
    assert [item.status for item in started.items] == [JobItemStatus.READY, JobItemStatus.READY]

    cancelled = service.cancel_job(user_id=1001, command=CancelJobCommand(job_id=job.id, reason="user_stop"))
    db_session.commit()

    assert cancelled is True


def test_job_application_service_rejects_start_for_other_user(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "items": [{"item_type": "card", "input_payload": {"item_name": "DarkBlade"}}],
            }
        ),
    )

    assert service.start_job(user_id=2002, command=StartJobCommand(job_id=job.id)) is None


def test_job_application_service_returns_existing_job_when_claimed_by_other_consumer(db_session, tmp_path):
    claim_service = ServerQueuedJobClaimService(storage_root=tmp_path / "queued-job-claims", lease_seconds=120)
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        server_queued_job_claim_service=claim_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "items": [{"item_type": "card", "input_payload": {"item_name": "DarkBlade"}}],
            }
        ),
    )
    existing_claim = claim_service.acquire_claim(job_id=job.id, owner_scope="external-worker")

    started, claimed, _, _ = service.start_job_attempt(
        user_id=1001,
        command=StartJobCommand(job_id=job.id, triggered_by="system_queue_worker"),
    )

    assert started is not None
    assert started.id == job.id
    assert started.status == JobStatus.DRAFT
    assert claimed is False

    claim_service.release_claim(existing_claim)


class _LogRegistry:
    def resolve(self, job_type: str, item_type: str):
        if (job_type, item_type) != ("log_analysis", "log_analysis"):
            raise KeyError(f"workflow not found for {job_type}/{item_type}")
        return [PlatformWorkflowStep(step_type="log.analyze", step_id="log-analyze")]


class _SupportedServerRegistry:
    def resolve(self, job_type: str, item_type: str, input_payload: dict[str, object] | None = None):
        payload = dict(input_payload or {})
        if (job_type, item_type) == ("log_analysis", "log_analysis"):
            return [PlatformWorkflowStep(step_type="log.analyze", step_id="log-analyze")]
        if (job_type, item_type) == ("batch_generate", "custom_code"):
            return [
                PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="batch-custom-code"),
                PlatformWorkflowStep(step_type="code.generate", step_id="batch-custom-codegen"),
                PlatformWorkflowStep(step_type="build.project", step_id="batch-custom-code-build"),
            ]
        if (job_type, item_type) == ("batch_generate", "card"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="batch-card-plan",
                    input_payload={"asset_type": "card"},
                )
            ]
        if (job_type, item_type) == ("batch_generate", "card_fullscreen"):
            if (
                str(payload.get("uploaded_asset_ref", "")).strip()
                and str(payload.get("server_project_ref", "")).strip()
            ):
                return [
                    PlatformWorkflowStep(
                        step_type="asset.generate",
                        step_id="batch-card-fullscreen-asset",
                        input_payload={"asset_type": "card_fullscreen"},
                    ),
                    PlatformWorkflowStep(step_type="build.project", step_id="batch-card-fullscreen-build"),
                ]
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="batch-card-fullscreen-plan",
                    input_payload={"asset_type": "card_fullscreen"},
                )
            ]
        if (job_type, item_type) == ("batch_generate", "relic"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="batch-relic-plan",
                    input_payload={"asset_type": "relic"},
                )
            ]
        if (job_type, item_type) == ("batch_generate", "power"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="batch-power-plan",
                    input_payload={"asset_type": "power"},
                )
            ]
        if (job_type, item_type) == ("batch_generate", "character"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="batch-character-plan",
                    input_payload={"asset_type": "character"},
                )
            ]
        if (job_type, item_type) == ("single_generate", "custom_code"):
            return [
                PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="single-custom-code"),
                PlatformWorkflowStep(step_type="code.generate", step_id="single-custom-codegen"),
                PlatformWorkflowStep(step_type="build.project", step_id="single-custom-code-build"),
            ]
        if (job_type, item_type) == ("single_generate", "card"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="single-card-plan",
                    input_payload={"asset_type": "card"},
                )
            ]
        if (job_type, item_type) == ("single_generate", "card_fullscreen"):
            if (
                str(payload.get("uploaded_asset_ref", "")).strip()
                and str(payload.get("server_project_ref", "")).strip()
            ):
                return [
                    PlatformWorkflowStep(
                        step_type="asset.generate",
                        step_id="single-card-fullscreen-asset",
                        input_payload={"asset_type": "card_fullscreen"},
                    ),
                    PlatformWorkflowStep(step_type="build.project", step_id="single-card-fullscreen-build"),
                ]
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="single-card-fullscreen-plan",
                    input_payload={"asset_type": "card_fullscreen"},
                )
            ]
        if (job_type, item_type) == ("single_generate", "relic"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="single-relic-plan",
                    input_payload={"asset_type": "relic"},
                )
            ]
        if (job_type, item_type) == ("single_generate", "power"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="single-power-plan",
                    input_payload={"asset_type": "power"},
                )
            ]
        if (job_type, item_type) == ("single_generate", "character"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="single-character-plan",
                    input_payload={"asset_type": "character"},
                )
            ]
        raise KeyError(f"workflow not found for {job_type}/{item_type}")


class _SucceededRunner:
    async def run(self, *, steps, base_request):
        results: list[StepExecutionResult] = []
        payload = dict(base_request.input_payload)
        for step in steps:
            merged = dict(payload)
            merged.update(step.input_payload)
            text = "分析完成"
            output_payload: dict[str, object] = {"text": text}
            if step.step_type == "batch.custom_code.plan":
                output_payload = {
                    "text": "已生成服务器 custom_code 实现方案",
                    "analysis": "摘要：建议先补一个 Harmony Patch 骨架",
                    "item_name": str(merged.get("item_name", "")).strip(),
                    "server_workspace_root": str(merged.get("server_workspace_root", "")).strip(),
                }
            elif step.step_type == "code.generate":
                output_payload = {
                    "text": f"已写入 {str(merged.get('item_name', '')).strip()} 的服务器 custom_code 代码"
                }
            elif step.step_type == "asset.generate":
                output_payload = {"text": f"已写入 {str(merged.get('item_name', '')).strip()} 的服务器资产代码"}
            elif step.step_type == "build.project":
                item_name = str(merged.get("item_name", "")).strip()
                output_payload = {
                    "text": f"已完成 {item_name} 的服务器项目构建",
                    "artifacts": [
                        {
                            "artifact_type": "build_output",
                            "storage_provider": "server_workspace",
                            "object_key": f"/runtime/{item_name}.dll",
                            "file_name": f"{item_name}.dll",
                            "mime_type": "application/octet-stream",
                            "size_bytes": 3,
                            "result_summary": "服务器构建产物",
                        }
                    ],
                }
            elif step.step_type == "single.asset.plan":
                asset_type = str(
                    step.input_payload.get("asset_type") or base_request.input_payload.get("asset_type", "")
                ).strip()
                if asset_type == "card":
                    text = "已生成服务器卡牌实现方案"
                elif asset_type == "card_fullscreen":
                    text = "已生成服务器全画面卡实现方案"
                elif asset_type == "character":
                    text = "已生成服务器角色实现方案"
                elif asset_type == "power":
                    text = "已生成服务器 Power 实现方案"
                else:
                    text = "已生成服务器遗物实现方案"
                output_payload = {"text": text}
            results.append(
                StepExecutionResult(
                    step_id=step.step_id,
                    status="succeeded",
                    output_payload=output_payload,
                )
            )
            payload.update(output_payload)
        return results


class _SwitchableDeployTargetRunner:
    def __init__(self) -> None:
        self.busy_targets: set[str] = set()

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
                output_payload = {
                    "text": f"已写入 {str(merged.get('item_name', '')).strip()} 的服务器 custom_code 代码"
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
            if step.step_type == "build.project":
                project_name = str(merged.get("server_project_name", "")).strip()
                if project_name in self.busy_targets:
                    results.append(
                        StepExecutionResult(
                            step_id=step.step_id,
                            status="failed_system",
                            error_summary="server deploy target is busy",
                            error_payload={
                                "reason_code": "server_deploy_target_busy",
                                "reason_message": "server deploy target is busy",
                                "resource_type": "server_deploy_target",
                                "resource_key": project_name,
                                "project_name": project_name,
                                "server_project_ref": str(merged.get("server_project_ref", "")).strip(),
                            },
                        )
                    )
                    break
                output_payload = {
                    "text": f"已完成 {str(merged.get('item_name', '')).strip()} 的服务器项目构建",
                    "deployed_to": f"/Mods/{project_name}",
                    "artifacts": [
                        {
                            "artifact_type": "deployed_output",
                            "storage_provider": "server_deploy",
                            "object_key": f"/Mods/{project_name}/{project_name}.dll",
                            "file_name": f"{project_name}.dll",
                            "mime_type": "application/octet-stream",
                            "size_bytes": 3,
                            "result_summary": "服务器部署产物",
                        }
                    ],
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


class _BusyWorkspaceLockService:
    def acquire_write_lock(self, **kwargs):
        raise ServerWorkspaceBusyError(
            "server workspace is busy",
            server_project_ref=str(kwargs.get("server_project_ref", "")),
        )

    def release_write_lock(self, handle):
        raise AssertionError("release_write_lock should not be called when acquire fails")


class _SwitchableWorkspaceLockService:
    def __init__(self) -> None:
        self.busy_refs: set[str] = set()

    def acquire_write_lock(self, **kwargs):
        server_project_ref = str(kwargs.get("server_project_ref", "")).strip()
        if server_project_ref in self.busy_refs:
            raise ServerWorkspaceBusyError(
                "server workspace is busy",
                server_project_ref=server_project_ref,
            )
        return object()

    def release_write_lock(self, handle):
        return None


class _CapturingRunner:
    def __init__(self) -> None:
        self.last_base_request = None

    async def run(self, *, steps, base_request):
        self.last_base_request = base_request
        return [
            StepExecutionResult(
                step_id=steps[0].step_id,
                status="succeeded",
                output_payload={"text": "captured"},
            )
        ]


class _RetryOnceBindingRunner:
    def __init__(self) -> None:
        self.binding_refs: list[str] = []

    async def run(self, *, steps, base_request):
        self.binding_refs.append(base_request.execution_binding.credential_ref)
        if len(self.binding_refs) == 1:
            return [
                StepExecutionResult(
                    step_id=steps[-1].step_id,
                    status="failed_system",
                    error_summary="401 invalid token",
                )
            ]
        return [
            StepExecutionResult(
                step_id=steps[-1].step_id,
                status="succeeded",
                output_payload={"text": "自动切换后执行成功"},
            )
        ]


def test_job_application_service_can_complete_supported_log_analysis_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "log_analysis",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "log_analysis",
                        "input_summary": "分析最近一次日志",
                        "input_payload": {"context": "黑屏了"},
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "分析完成"


def test_job_application_service_can_complete_supported_batch_custom_code_job(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "补一个战斗脚本管理器",
                        "input_payload": {
                            "item_name": "BattleScriptManager",
                            "description": "实现一个战斗阶段脚本管理器",
                            "implementation_notes": "维护状态机并派发事件",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已完成 BattleScriptManager 的服务器项目构建"


def test_job_application_service_can_complete_supported_batch_card_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card",
                        "input_summary": "补一个批量卡牌实现方案",
                        "input_payload": {
                            "item_name": "DarkBlade",
                            "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器卡牌实现方案"


def test_job_application_service_can_complete_supported_batch_card_fullscreen_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card_fullscreen",
                        "input_summary": "补一个批量全画面卡实现方案",
                        "input_payload": {
                            "item_name": "DarkBladeFullscreen",
                            "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器全画面卡实现方案"


def test_job_application_service_can_complete_batch_card_fullscreen_with_uploaded_asset(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    uploaded_asset_service = UploadedAssetService(storage_root=tmp_path / "platform-upload-assets")
    uploaded = uploaded_asset_service.create_asset(
        user_id=1001,
        file_name="fullscreen.png",
        content_base64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y3ioAAAAASUVORK5CYII=",
        mime_type="image/png",
    )

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        uploaded_asset_service=uploaded_asset_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
        uploaded_asset_service=uploaded_asset_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card_fullscreen",
                        "input_summary": "补一个批量全画面卡实现方案",
                        "input_payload": {
                            "item_name": "DarkBladeFullscreen",
                            "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                            "image_mode": "upload",
                            "uploaded_asset_ref": uploaded.uploaded_asset_ref,
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已完成 DarkBladeFullscreen 的服务器项目构建"


def test_job_application_service_can_complete_supported_batch_relic_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "relic",
                        "input_summary": "补一个批量遗物实现方案",
                        "input_payload": {
                            "item_name": "FangedGrimoire",
                            "description": "每次造成伤害时获得 2 点格挡。",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器遗物实现方案"


def test_job_application_service_can_complete_supported_batch_power_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "power",
                        "input_summary": "补一个批量 Power 实现方案",
                        "input_payload": {
                            "item_name": "CorruptionBuff",
                            "description": "每层在回合结束时额外造成 1 点伤害，最多叠加 10 层。",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器 Power 实现方案"


def test_job_application_service_can_complete_supported_batch_character_job(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "batch_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "character",
                        "input_summary": "补一个批量角色实现方案",
                        "input_payload": {
                            "item_name": "WatcherAlpha",
                            "description": "一名偏进攻型角色，初始拥有额外 1 点能量。",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器角色实现方案"


def test_job_application_service_can_complete_supported_single_custom_code_job(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "补一个单资产脚本",
                        "input_payload": {
                            "item_name": "SingleEffectPatch",
                            "description": "补一个单资产 custom_code 示例",
                            "image_mode": "ai",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已完成 SingleEffectPatch 的服务器项目构建"
    artifacts = ArtifactRepositorySqlAlchemy(db_session).list_by_job_item(started.items[0].id)
    assert artifacts[0].file_name == "SingleEffectPatch.dll"


def test_job_application_service_keeps_job_queued_when_server_workspace_is_busy(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        server_workspace_lock_service=_BusyWorkspaceLockService(),
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "补一个单资产脚本",
                        "input_payload": {
                            "item_name": "SingleEffectPatch",
                            "description": "补一个单资产 custom_code 示例",
                            "image_mode": "ai",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.QUEUED
    assert started.items[0].status == JobItemStatus.READY
    assert started.items[0].error_summary == "server workspace is busy"
    assert started.error_summary == "server workspace is busy"
    assert db_session.query(AIExecutionRecord).count() == 0

    queued_event = (
        db_session.query(JobEventRecord)
        .filter(JobEventRecord.job_id == job.id, JobEventRecord.event_type == "job.queued")
        .order_by(JobEventRecord.id.desc())
        .first()
    )
    assert queued_event is not None
    assert queued_event.job_item_id == started.items[0].id
    assert queued_event.event_payload["reason_code"] == "server_workspace_busy"
    assert queued_event.event_payload["reason_message"] == "server workspace is busy"


def test_job_application_service_auto_resumes_next_queued_job_after_workspace_is_released(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    lock_service = _SwitchableWorkspaceLockService()
    lock_service.busy_refs.add(workspace.server_project_ref)

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_lock_service=lock_service,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )

    queued_job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "排队任务",
                        "input_payload": {
                            "item_name": "QueuedPatch",
                            "description": "等待工作区释放",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )
    queued_started = service.start_job(user_id=1001, command=StartJobCommand(job_id=queued_job.id))
    assert queued_started is not None
    assert queued_started.status == JobStatus.QUEUED

    lock_service.busy_refs.clear()

    active_job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "当前任务",
                        "input_payload": {
                            "item_name": "ActivePatch",
                            "description": "当前持有工作区锁并完成",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )
    active_started = service.start_job(user_id=1001, command=StartJobCommand(job_id=active_job.id))
    assert active_started is not None
    assert active_started.status == JobStatus.SUCCEEDED

    reloaded_queued = JobRepositorySqlAlchemy(db_session).find_by_id_for_user(queued_job.id, 1001)
    assert reloaded_queued is not None
    assert reloaded_queued.status == JobStatus.SUCCEEDED
    assert reloaded_queued.items[0].status == JobItemStatus.SUCCEEDED
    assert reloaded_queued.items[0].result_summary == "已完成 QueuedPatch 的服务器项目构建"

    resume_event = (
        db_session.query(JobEventRecord)
        .filter(JobEventRecord.job_id == queued_job.id, JobEventRecord.event_type == "job.queued")
        .order_by(JobEventRecord.id.desc())
        .first()
    )
    assert resume_event is not None
    assert resume_event.event_payload["triggered_by"] == "system_workspace_resume"


def test_job_application_service_keeps_job_queued_when_server_deploy_target_is_busy(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    runner = _SwitchableDeployTargetRunner()
    runner.busy_targets.add("DarkMod")

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=runner,
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "补一个单资产脚本",
                        "input_payload": {
                            "item_name": "SingleEffectPatch",
                            "description": "补一个单资产 custom_code 示例",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.QUEUED
    assert started.items[0].status == JobItemStatus.READY
    assert started.items[0].error_summary == "server deploy target is busy"
    assert started.error_summary == "server deploy target is busy"

    queued_event = (
        db_session.query(JobEventRecord)
        .filter(JobEventRecord.job_id == job.id, JobEventRecord.event_type == "job.queued")
        .order_by(JobEventRecord.id.desc())
        .first()
    )
    assert queued_event is not None
    assert queued_event.job_item_id == started.items[0].id
    assert queued_event.event_payload["reason_code"] == "server_deploy_target_busy"
    assert queued_event.event_payload["reason_message"] == "server deploy target is busy"
    assert queued_event.event_payload["resource_key"] == "DarkMod"


def test_job_application_service_auto_resumes_next_queued_job_after_deploy_target_is_released(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    for user_id in (1001, 1002):
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    leader_workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    queued_workspace = workspace_service.create_workspace(user_id=1002, project_name="DarkMod")
    runner = _SwitchableDeployTargetRunner()
    runner.busy_targets.add("DarkMod")

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=runner,
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )

    queued_job = service.create_job(
        user_id=1002,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "排队等待同名部署目录",
                        "input_payload": {
                            "item_name": "QueuedEffectPatch",
                            "description": "等待同名部署目录释放后再继续",
                            "server_project_ref": queued_workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )
    queued_started = service.start_job(user_id=1002, command=StartJobCommand(job_id=queued_job.id))
    assert queued_started is not None
    assert queued_started.status == JobStatus.QUEUED

    runner.busy_targets.clear()
    leader_job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "先完成当前部署链",
                        "input_payload": {
                            "item_name": "LeaderEffectPatch",
                            "description": "释放同名部署目录占用",
                            "server_project_ref": leader_workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    leader_started = service.start_job(user_id=1001, command=StartJobCommand(job_id=leader_job.id))

    assert leader_started is not None
    assert leader_started.status == JobStatus.SUCCEEDED
    db_session.expire_all()
    resumed_job = JobRepositorySqlAlchemy(db_session).find_by_id_for_user(queued_job.id, 1002)
    assert resumed_job is not None
    assert resumed_job.status == JobStatus.SUCCEEDED
    assert resumed_job.items[0].status == JobItemStatus.SUCCEEDED
    assert resumed_job.items[0].result_summary == "已完成 QueuedEffectPatch 的服务器项目构建"


def test_job_application_service_can_complete_supported_single_relic_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "relic",
                        "input_summary": "补一个遗物实现方案",
                        "input_payload": {
                            "asset_type": "relic",
                            "item_name": "FangedGrimoire",
                            "description": "每次造成伤害时获得 2 点格挡。",
                            "image_mode": "ai",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器遗物实现方案"


def test_job_application_service_can_complete_supported_single_card_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card",
                        "input_summary": "补一个卡牌实现方案",
                        "input_payload": {
                            "item_name": "DarkBlade",
                            "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                            "image_mode": "ai",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器卡牌实现方案"


def test_job_application_service_can_complete_supported_single_card_fullscreen_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card_fullscreen",
                        "input_summary": "补一个全画面卡实现方案",
                        "input_payload": {
                            "item_name": "DarkBladeFullscreen",
                            "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                            "image_mode": "ai",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器全画面卡实现方案"


def test_job_application_service_can_complete_single_card_fullscreen_with_uploaded_asset(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    uploaded_asset_service = UploadedAssetService(storage_root=tmp_path / "platform-upload-assets")
    uploaded = uploaded_asset_service.create_asset(
        user_id=1001,
        file_name="fullscreen.png",
        content_base64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y3ioAAAAASUVORK5CYII=",
        mime_type="image/png",
    )

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        artifact_repository=ArtifactRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        uploaded_asset_service=uploaded_asset_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
        uploaded_asset_service=uploaded_asset_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card_fullscreen",
                        "input_summary": "补一个全画面卡实现方案",
                        "input_payload": {
                            "item_name": "DarkBladeFullscreen",
                            "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                            "image_mode": "upload",
                            "uploaded_asset_ref": uploaded.uploaded_asset_ref,
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已完成 DarkBladeFullscreen 的服务器项目构建"


def test_job_application_service_can_complete_supported_single_power_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "power",
                        "input_summary": "补一个 Power 实现方案",
                        "input_payload": {
                            "asset_type": "power",
                            "item_name": "CorruptionBuff",
                            "description": "每层在回合结束时额外造成 1 点伤害，最多叠加 10 层。",
                            "image_mode": "ai",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器 Power 实现方案"


def test_job_application_service_can_complete_supported_single_character_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "character",
                        "input_summary": "补一个角色实现方案",
                        "input_payload": {
                            "asset_type": "character",
                            "item_name": "WatcherAlpha",
                            "description": "一名偏进攻型角色，初始拥有额外 1 点能量。",
                            "image_mode": "ai",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "已生成服务器角色实现方案"


def test_job_application_service_rejects_platform_payload_with_legacy_name_field(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    try:
        service.create_job(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "batch_generate",
                    "workflow_version": "2026.03.31",
                    "items": [
                        {
                            "item_type": "card",
                            "input_summary": "补一个批量卡牌实现方案",
                            "input_payload": {
                                "name": "DarkBlade",
                                "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                            },
                        }
                    ],
                }
            ),
        )
    except ValueError as error:
        assert str(error) == "platform job payload for batch_generate/card contains forbidden fields: name"
    else:
        raise AssertionError("expected ValueError when legacy name field is present")


def test_job_application_service_rejects_platform_payload_with_project_root_and_upload_flags(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    try:
        service.create_job(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "single_generate",
                    "workflow_version": "2026.03.31",
                    "items": [
                        {
                            "item_type": "relic",
                            "input_summary": "Dark Relic",
                            "input_payload": {
                                "asset_type": "relic",
                                "item_name": "DarkRelic",
                                "description": "每次造成伤害时获得 2 点格挡。",
                                "project_root": "E:/STS2mod/MyMod",
                                "has_uploaded_image": True,
                            },
                        }
                    ],
                }
            ),
        )
    except ValueError as error:
        assert str(error) == (
            "platform job payload for single_generate/relic contains forbidden fields: "
            "has_uploaded_image, project_root"
        )
    else:
        raise AssertionError("expected ValueError when forbidden platform payload fields are present")


def test_job_application_service_requires_server_project_ref_for_single_custom_code(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    try:
        service.create_job(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "single_generate",
                    "workflow_version": "2026.03.31",
                    "items": [
                        {
                            "item_type": "custom_code",
                            "input_summary": "补一个单资产脚本",
                            "input_payload": {
                                "item_name": "SingleEffectPatch",
                                "description": "补一个单资产 custom_code 示例",
                            },
                        }
                    ],
                }
            ),
        )
    except ValueError as error:
        assert str(error) == "platform job payload for single_generate/custom_code requires server_project_ref"
    else:
        raise AssertionError("expected ValueError when server_project_ref is missing")


def test_job_application_service_requires_server_project_ref_for_batch_custom_code(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    try:
        service.create_job(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "batch_generate",
                    "workflow_version": "2026.03.31",
                    "items": [
                        {
                            "item_type": "custom_code",
                            "input_summary": "补一个战斗脚本管理器",
                            "input_payload": {
                                "item_name": "BattleScriptManager",
                                "description": "实现一个战斗阶段脚本管理器",
                            },
                        }
                    ],
                }
            ),
        )
    except ValueError as error:
        assert str(error) == "platform job payload for batch_generate/custom_code requires server_project_ref"
    else:
        raise AssertionError("expected ValueError when batch custom_code server_project_ref is missing")


def test_job_application_service_requires_server_project_ref_for_single_card_fullscreen_with_uploaded_asset(db_session):
    uploaded_asset_service = UploadedAssetService(
        storage_root=Path(db_session.bind.url.database).parent / "platform-upload-assets"
    )
    uploaded = uploaded_asset_service.create_asset(
        user_id=1001,
        file_name="fullscreen.png",
        content_base64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y3ioAAAAASUVORK5CYII=",
        mime_type="image/png",
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        uploaded_asset_service=uploaded_asset_service,
    )

    try:
        service.create_job(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "single_generate",
                    "workflow_version": "2026.03.31",
                    "items": [
                        {
                            "item_type": "card_fullscreen",
                            "input_summary": "补一个全画面卡实现方案",
                            "input_payload": {
                                "asset_type": "card_fullscreen",
                                "item_name": "DarkBladeFullscreen",
                                "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                                "uploaded_asset_ref": uploaded.uploaded_asset_ref,
                            },
                        }
                    ],
                }
            ),
        )
    except ValueError as error:
        assert (
            str(error)
            == "platform job payload for single_generate/card_fullscreen requires server_project_ref when uploaded_asset_ref is present"
        )
    else:
        raise AssertionError("expected ValueError when card_fullscreen uploaded asset has no server_project_ref")


def test_job_application_service_requires_server_project_ref_for_batch_card_fullscreen_with_uploaded_asset(db_session):
    uploaded_asset_service = UploadedAssetService(
        storage_root=Path(db_session.bind.url.database).parent / "platform-upload-assets"
    )
    uploaded = uploaded_asset_service.create_asset(
        user_id=1001,
        file_name="fullscreen.png",
        content_base64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y3ioAAAAASUVORK5CYII=",
        mime_type="image/png",
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        uploaded_asset_service=uploaded_asset_service,
    )

    try:
        service.create_job(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "batch_generate",
                    "workflow_version": "2026.03.31",
                    "items": [
                        {
                            "item_type": "card_fullscreen",
                            "input_summary": "补一个批量全画面卡实现方案",
                            "input_payload": {
                                "asset_type": "card_fullscreen",
                                "item_name": "DarkBladeFullscreen",
                                "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                                "uploaded_asset_ref": uploaded.uploaded_asset_ref,
                            },
                        }
                    ],
                }
            ),
        )
    except ValueError as error:
        assert (
            str(error)
            == "platform job payload for batch_generate/card_fullscreen requires server_project_ref when uploaded_asset_ref is present"
        )
    else:
        raise AssertionError("expected ValueError when batch card_fullscreen uploaded asset has no server_project_ref")


def test_job_application_service_accepts_uploaded_asset_ref_for_platform_payload(db_session, tmp_path):
    uploaded_asset_service = UploadedAssetService(storage_root=tmp_path / "platform-upload-assets")
    uploaded = uploaded_asset_service.create_asset(
        user_id=1001,
        file_name="dark-blade.png",
        content_base64="ZmFrZS1pbWFnZS1ieXRlcw==",
        mime_type="image/png",
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        uploaded_asset_service=uploaded_asset_service,
    )

    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "items": [
                    {
                        "item_type": "card",
                        "input_summary": "补一个卡牌实现方案",
                        "input_payload": {
                            "asset_type": "card",
                            "item_name": "DarkBlade",
                            "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                            "uploaded_asset_ref": uploaded.uploaded_asset_ref,
                        },
                    }
                ],
            }
        ),
    )

    assert job.status == JobStatus.DRAFT


def test_job_application_service_accepts_server_project_ref_for_platform_payload(db_session, tmp_path):
    server_workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = server_workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        server_workspace_service=server_workspace_service,
    )

    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "补一个单资产脚本",
                        "input_payload": {
                            "item_name": "SingleEffectPatch",
                            "description": "补一个单资产 custom_code 示例",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    assert job.status == JobStatus.DRAFT


def test_execution_orchestrator_service_hydrates_server_workspace_metadata_into_runner_payload(db_session, tmp_path):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    workspace_service = ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces")
    workspace = workspace_service.create_workspace(user_id=1001, project_name="DarkMod")
    capturing_runner = _CapturingRunner()

    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        server_workspace_service=workspace_service,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=capturing_runner,
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
        server_workspace_service=workspace_service,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "custom_code",
                        "input_summary": "补一个单资产脚本",
                        "input_payload": {
                            "item_name": "SingleEffectPatch",
                            "description": "补一个单资产 custom_code 示例",
                            "server_project_ref": workspace.server_project_ref,
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert capturing_runner.last_base_request is not None
    assert capturing_runner.last_base_request.input_payload["server_project_name"] == "DarkMod"
    assert "platform-workspaces" in str(capturing_runner.last_base_request.input_payload["server_workspace_root"])


def test_job_application_service_can_retry_with_alternate_credential_after_retryable_failure(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
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
    runner = _RetryOnceBindingRunner()
    orchestrator = ExecutionOrchestratorService(
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
        server_credential_cipher=cipher,
        workflow_registry=_SupportedServerRegistry(),
        workflow_runner=runner,
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card",
                        "input_summary": "补一个卡牌实现方案",
                        "input_payload": {
                            "item_name": "DarkBlade",
                            "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                            "image_mode": "ai",
                        },
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "自动切换后执行成功"
    assert runner.binding_refs == [
        f"server-credential:{credential_a.id}",
        f"server-credential:{credential_b.id}",
    ]


def test_job_application_service_rejects_start_when_active_server_job_limit_is_reached(db_session):
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
    repository = JobRepositorySqlAlchemy(db_session)
    for item_name in ("BusyA", "BusyB"):
        job = repository.create_job_with_items(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "single_generate",
                    "workflow_version": "2026.03.31",
                    "selected_execution_profile_id": profile.id,
                    "selected_agent_backend": "codex",
                    "selected_model": "gpt-5.4",
                    "items": [{"item_type": "card", "input_payload": {"item_name": item_name}}],
                }
            ),
        )
        job.status = JobStatus.RUNNING
    target_job = repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [{"item_type": "card", "input_payload": {"item_name": "Target"}}],
            }
        ),
    )
    service = JobApplicationService(
        job_repository=repository,
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    try:
        service.start_job(user_id=1001, command=StartJobCommand(job_id=target_job.id))
    except ValueError as error:
        assert str(error) == "too many active server jobs for user: limit 2"
    else:
        raise AssertionError("expected ValueError when active server job limit is reached")
