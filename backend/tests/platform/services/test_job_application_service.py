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
from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService
from app.modules.platform.application.services.uploaded_asset_service import UploadedAssetService
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
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
from app.modules.platform.infra.persistence.repositories.usage_ledger_repository_sqlalchemy import UsageLedgerRepositorySqlAlchemy
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


class _LogRegistry:
    def resolve(self, job_type: str, item_type: str):
        if (job_type, item_type) != ("log_analysis", "log_analysis"):
            raise KeyError(f"workflow not found for {job_type}/{item_type}")
        return [PlatformWorkflowStep(step_type="log.analyze", step_id="log-analyze")]


class _SupportedServerRegistry:
    def resolve(self, job_type: str, item_type: str):
        if (job_type, item_type) == ("log_analysis", "log_analysis"):
            return [PlatformWorkflowStep(step_type="log.analyze", step_id="log-analyze")]
        if (job_type, item_type) == ("batch_generate", "custom_code"):
            return [PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="batch-custom-code")]
        if (job_type, item_type) == ("batch_generate", "card"):
            return [
                PlatformWorkflowStep(
                    step_type="single.asset.plan",
                    step_id="batch-card-plan",
                    input_payload={"asset_type": "card"},
                )
            ]
        if (job_type, item_type) == ("batch_generate", "card_fullscreen"):
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
            return [PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="single-custom-code")]
        if (job_type, item_type) == ("single_generate", "card"):
            return [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single-card-plan")]
        if (job_type, item_type) == ("single_generate", "card_fullscreen"):
            return [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single-card-fullscreen-plan")]
        if (job_type, item_type) == ("single_generate", "relic"):
            return [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single-relic-plan")]
        if (job_type, item_type) == ("single_generate", "power"):
            return [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single-power-plan")]
        if (job_type, item_type) == ("single_generate", "character"):
            return [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single-character-plan")]
        raise KeyError(f"workflow not found for {job_type}/{item_type}")


class _SucceededRunner:
    async def run(self, *, steps, base_request):
        text = "分析完成"
        if steps[0].step_type == "batch.custom_code.plan":
            text = "已生成服务器 custom_code 实现方案"
        if steps[0].step_type == "single.asset.plan":
            asset_type = str(steps[0].input_payload.get("asset_type") or base_request.input_payload.get("asset_type", "")).strip()
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
        return [
            StepExecutionResult(
                step_id=steps[0].step_id,
                status="succeeded",
                output_payload={"text": text},
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


def test_job_application_service_can_complete_supported_batch_custom_code_job(db_session):
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
                        "item_type": "custom_code",
                        "input_summary": "补一个战斗脚本管理器",
                        "input_payload": {
                            "item_name": "BattleScriptManager",
                            "description": "实现一个战斗阶段脚本管理器",
                            "implementation_notes": "维护状态机并派发事件",
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
    assert started.items[0].result_summary == "已生成服务器 custom_code 实现方案"


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


def test_job_application_service_can_complete_supported_batch_character_job(db_session):
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


def test_job_application_service_can_complete_supported_single_custom_code_job(db_session):
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
                        "item_type": "custom_code",
                        "input_summary": "补一个单资产脚本",
                        "input_payload": {
                            "item_name": "SingleEffectPatch",
                            "description": "补一个单资产 custom_code 示例",
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
    assert started.items[0].result_summary == "已生成服务器 custom_code 实现方案"


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
                            "asset_type": "card",
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
                            "asset_type": "card_fullscreen",
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
