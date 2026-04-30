from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ArtifactRecord,
    ExecutionChargeRecord,
    JobEventRecord,
    QuotaAccountRecord,
    QuotaBucketRecord,
)
from app.modules.platform.infra.persistence.repositories.admin_query_repositories_sqlalchemy import (
    AdminQueryRepositoriesSqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_query_repository_sqlalchemy import (
    JobQueryRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.modules.platform.infra.persistence.repositories.quota_query_repository_sqlalchemy import (
    QuotaQueryRepositorySqlAlchemy,
)


def _seed_query_fixture(db_session):
    job_repository = JobRepositorySqlAlchemy(db_session)
    command = CreateJobCommand.model_validate(
        {
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "批量任务",
            "selected_execution_profile_id": 7,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
            "items": [
                {"item_type": "card", "input_summary": "卡牌一", "input_payload": {"name": "One"}},
            ],
        }
    )
    job = job_repository.create_job_with_items(user_id=1001, command=command)
    other_job = job_repository.create_job_with_items(user_id=2002, command=command)
    db_session.flush()

    item = job.items[0]
    execution = AIExecutionRecord(
        job_id=job.id,
        job_item_id=item.id,
        user_id=1001,
        status="succeeded",
        provider="openai",
        model="gpt-5.4",
        request_idempotency_key="idem-admin",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-admin",
        result_summary="ok",
    )
    artifact = ArtifactRecord(
        job_id=job.id,
        job_item_id=item.id,
        user_id=1001,
        artifact_type="image",
        storage_provider="local",
        object_key="result/a.png",
        file_name="a.png",
        result_summary="生成成功",
    )
    build_artifact = ArtifactRecord(
        job_id=job.id,
        job_item_id=item.id,
        user_id=1001,
        artifact_type="build_output",
        storage_provider="server_workspace",
        object_key="/runtime/MyMod.dll",
        file_name="MyMod.dll",
        result_summary="服务器构建产物",
    )
    deployed_artifact = ArtifactRecord(
        job_id=job.id,
        job_item_id=item.id,
        user_id=1001,
        artifact_type="deployed_output",
        storage_provider="server_deploy",
        object_key="/game/Mods/MyMod/MyMod.dll",
        file_name="MyMod.dll",
        result_summary="服务器部署产物",
    )
    event = JobEventRecord(
        job_id=job.id,
        job_item_id=item.id,
        ai_execution_id=None,
        user_id=1001,
        event_type="job.succeeded",
        event_payload={"status": "succeeded"},
    )
    refund = ExecutionChargeRecord(
        ai_execution_id=1,
        user_id=1001,
        charge_status="refunded",
        charge_amount=1,
        refund_reason="system_error",
    )
    quota_account = QuotaAccountRecord(user_id=1001, status="active")
    db_session.add_all([execution, artifact, build_artifact, deployed_artifact, event, quota_account])
    db_session.flush()
    refund.ai_execution_id = execution.id
    db_session.add(refund)
    db_session.add_all(
        [
            QuotaBucketRecord(
                quota_account_id=quota_account.id,
                bucket_type="daily",
                period_start=datetime.now(UTC) - timedelta(hours=1),
                period_end=datetime.now(UTC) + timedelta(hours=23),
                quota_limit=10,
                used_amount=3,
                refunded_amount=1,
            ),
            QuotaBucketRecord(
                quota_account_id=quota_account.id,
                bucket_type="weekly",
                period_start=datetime.now(UTC) - timedelta(days=1),
                period_end=datetime.now(UTC) + timedelta(days=6),
                quota_limit=50,
                used_amount=8,
                refunded_amount=1,
            ),
        ]
    )
    db_session.commit()
    return job, other_job, execution


def test_job_query_repository_returns_user_scoped_views(db_session):
    job, other_job, _ = _seed_query_fixture(db_session)
    repository = JobQueryRepositorySqlAlchemy(db_session)

    jobs = repository.list_jobs(1001)
    detail = repository.get_job_detail(1001, job.id)
    items = repository.list_job_items(1001, job.id)
    events = repository.list_visible_events(1001, job.id, after_id=None, limit=10)

    assert [entry.id for entry in jobs] == [job.id]
    assert all(entry.id != other_job.id for entry in jobs)
    assert detail is not None
    assert detail.selected_execution_profile_id == 7
    assert detail.selected_agent_backend == "codex"
    assert detail.selected_model == "gpt-5.4"
    assert detail.delivery_state == "deployed"
    assert detail.items[0].delivery_state == "deployed"
    assert detail.artifacts[0].file_name == "a.png"
    assert detail.artifacts[0].storage_provider == "local"
    assert detail.artifacts[0].object_key == "result/a.png"
    assert items[0].item_type == "card"
    assert events[0].as_user_payload()["event_type"] == "job.succeeded"


def test_quota_and_admin_query_repositories_return_split_views(db_session):
    job, _, execution = _seed_query_fixture(db_session)
    quota_repository = QuotaQueryRepositorySqlAlchemy(db_session)
    admin_repository = AdminQueryRepositoriesSqlAlchemy(db_session)

    quota = quota_repository.get_current_quota_view(1001, datetime.now(UTC))
    executions = admin_repository.list_executions(job_id=job.id)
    execution_detail = admin_repository.get_execution_detail(execution.id)
    refunds = admin_repository.list_refunds(user_id=1001)
    audit_events = admin_repository.list_audit_events(job_id=job.id)

    assert quota.total_limit == 10
    # quota view 来自 daily bucket（used=3, refunded=1），不是 weekly（used=8）
    assert quota.used_amount == 3
    assert quota.refunded_amount == 1
    assert executions[0].job_id == job.id
    assert execution_detail.request_idempotency_key == "idem-admin"
    assert refunds[0].refund_reason == "system_error"
    assert audit_events[0].event_type == "job.succeeded"
