from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

pytest.importorskip("sqlalchemy")
pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.application.services.server_workspace_lock_service import (
    ServerWorkspaceBusyError,
)
from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService
from app.modules.platform.application.services.uploaded_asset_service import UploadedAssetService
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionProfileRecord,
    JobItemRecord,
    JobRecord,
    QuotaAccountRecord,
    QuotaBucketRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.shared.infra.db.base import Base
from routers.auth_router import router as auth_router
from routers.platform_jobs import router as platform_router


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "platform-jobs.sqlite3"
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": f"sqlite+pysqlite:///{db_path.as_posix()}",
            },
            "auth": {
                "session_secret": "test-session-secret",
            },
        },
        runtime_role="web",
    )
    session = container.resolve_singleton("platform.db_session_factory")()
    Base.metadata.create_all(session.bind)
    session.commit()
    session.close()

    app = FastAPI()
    app.state.container = container
    container.register_singleton(
        "platform.server_workspace_service_factory",
        lambda: ServerWorkspaceService(storage_root=tmp_path / "platform-workspaces"),
    )
    container.register_singleton(
        "platform.uploaded_asset_service_factory",
        lambda: UploadedAssetService(storage_root=tmp_path / "platform-upload-assets"),
    )
    app.include_router(auth_router, prefix="/api")
    app.include_router(platform_router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def _register_login_and_verify(client: TestClient, username: str, email: str) -> int:
    registered = client.post(
        "/api/auth/register",
        json={
            "username": username,
            "email": email,
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    user_id = registered.json()["user"]["user_id"]
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": username,
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status="active")
        session.add(account)
        session.flush()
        session.add(
            QuotaBucketRecord(
                quota_account_id=account.id,
                bucket_type="daily",
                period_start=datetime.now(UTC) - timedelta(hours=1),
                period_end=datetime.now(UTC) + timedelta(hours=23),
                quota_limit=10,
                used_amount=0,
                refunded_amount=0,
            )
        )
        session.commit()
    finally:
        session.close()

    return user_id


def _seed_execution_profile(client: TestClient) -> int:
    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    settings = client.app.state.container.resolve_singleton("settings")
    cipher = ServerCredentialCipher.from_settings(settings)
    try:
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
        session.add(profile)
        session.flush()
        session.add(
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
        session.commit()
        return profile.id
    finally:
        session.close()


class _SucceededWorkflowRunner:
    def __init__(self, dispatcher=None) -> None:
        self.dispatcher = dispatcher

    async def run(self, *, steps, base_request):
        results: list[StepExecutionResult] = []
        payload = dict(base_request.input_payload)
        for step in steps:
            merged = dict(payload)
            merged.update(step.input_payload)
            text = "日志分析完成"
            output_payload: dict[str, object] = {"text": text}
            if step.step_type == "batch.custom_code.plan":
                output_payload = {
                    "text": "已生成服务器 custom_code 实现方案",
                    "analysis": "摘要：建议先补一个 Harmony Patch 骨架",
                    "item_name": str(merged.get("item_name", "")).strip(),
                    "server_workspace_root": str(merged.get("server_workspace_root", "")).strip(),
                }
            elif step.step_type == "code.generate":
                output_payload = {"text": f"已写入 {str(merged.get('item_name', '')).strip()} 的服务器 custom_code 代码"}
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
                asset_type = str(step.input_payload.get("asset_type") or base_request.input_payload.get("asset_type", "")).strip()
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
                    error_summary="",
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


def test_platform_jobs_router_supports_create_start_cancel_and_queries_for_current_session_user(client: TestClient):
    current_user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    other_user_id = _register_login_and_verify(client, "mira", "mira@example.com")
    assert other_user_id != current_user_id
    profile_id = _seed_execution_profile(client)

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    created = client.post(
        "/api/platform/jobs",
        params={"user_id": other_user_id},
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
            "items": [{"item_type": "potion", "input_payload": {"item_name": "DarkPotion"}}],
        },
    )
    assert created.status_code == 200
    payload = created.json()
    job_id = payload["id"]
    assert payload["status"] == "draft"
    assert payload["selected_execution_profile_id"] == profile_id
    assert payload["selected_agent_backend"] == "codex"
    assert payload["selected_model"] == "gpt-5.4"

    started = client.post(f"/api/platform/jobs/{job_id}/start", params={"user_id": other_user_id}, json={})
    assert started.status_code == 200
    assert started.json()["status"] == "deferred"

    listed = client.get("/api/platform/jobs", params={"user_id": other_user_id})
    assert listed.status_code == 200
    assert listed.json()[0]["id"] == job_id

    detail = client.get(f"/api/platform/jobs/{job_id}", params={"user_id": other_user_id})
    listed = client.get("/api/platform/jobs", params={"user_id": other_user_id})
    assert detail.status_code == 200
    assert detail.json()["id"] == job_id
    assert detail.json()["status"] == "deferred"
    assert detail.json()["selected_execution_profile_id"] == profile_id
    assert detail.json()["selected_agent_backend"] == "codex"
    assert detail.json()["selected_model"] == "gpt-5.4"
    assert detail.json()["original_deducted"] == 1
    assert detail.json()["refunded_amount"] == 1
    assert detail.json()["net_consumed"] == 0
    assert detail.json()["deferred_reason_code"] == "workflow_not_registered"
    assert "尚未为 single_generate/potion 注册" in detail.json()["deferred_reason_message"]
    assert listed.status_code == 200
    assert listed.json()[0]["original_deducted"] == 1
    assert listed.json()[0]["refunded_amount"] == 1
    assert listed.json()[0]["net_consumed"] == 0
    assert listed.json()[0]["deferred_reason_code"] == "workflow_not_registered"
    assert "尚未为 single_generate/potion 注册" in listed.json()[0]["deferred_reason_message"]

    items = client.get(f"/api/platform/jobs/{job_id}/items", params={"user_id": other_user_id})
    assert items.status_code == 200
    assert items.json()[0]["item_type"] == "potion"
    assert items.json()[0]["status"] == "deferred"

    events = client.get(f"/api/platform/jobs/{job_id}/events", params={"user_id": other_user_id})
    assert events.status_code == 200
    event_types = [entry["event_type"] for entry in events.json()]
    assert "job.created" in event_types
    assert "job.queued" in event_types
    assert "ai_execution.started" in event_types
    assert "ai_execution.deferred" in event_types
    assert "ai_execution.finished" in event_types

    deferred_event = next(entry for entry in events.json() if entry["event_type"] == "ai_execution.deferred")
    assert deferred_event["payload"]["reason_code"] == "workflow_not_registered"
    finished_event = next(entry for entry in events.json() if entry["event_type"] == "ai_execution.finished")
    assert finished_event["payload"]["status"] == "completed_with_refund"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "completed_with_refund"
        assert execution.provider == "openai"
        assert execution.model == "gpt-5.4"
        assert execution.credential_ref == "server-credential:1"
    finally:
        session.close()

    quota = client.get("/api/platform/quota", params={"user_id": other_user_id})
    assert quota.status_code == 200
    assert quota.json()["daily_limit"] == 10
    assert quota.json()["refunded"] == 1

    cancelled = client.post(
        f"/api/platform/jobs/{job_id}/cancel",
        params={"user_id": other_user_id},
        json={"reason": "stop"},
    )
    assert cancelled.status_code == 200
    assert cancelled.json()["ok"] is True


def test_platform_jobs_router_uses_current_user_default_server_profile_when_request_omits_selection(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        session.add(
            UserPlatformPreferenceRecord(
                user_id=user_id,
                default_execution_profile_id=profile_id,
            )
        )
        session.commit()
    finally:
        session.close()

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "items": [{"item_type": "character", "input_payload": {"item_name": "DarkRelic"}}],
        },
    )

    assert created.status_code == 200
    assert created.json()["selected_execution_profile_id"] == profile_id
    assert created.json()["selected_agent_backend"] == "codex"
    assert created.json()["selected_model"] == "gpt-5.4"


def test_platform_jobs_router_rejects_legacy_platform_payload_fields(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
            "items": [
                {
                    "item_type": "card",
                    "input_summary": "补一个卡牌实现方案",
                    "input_payload": {
                        "asset_type": "card",
                        "asset_name": "DarkBlade",
                        "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                    },
                }
            ],
        },
    )

    assert created.status_code == 400
    assert created.json()["detail"] == "platform job payload for single_generate/card contains forbidden fields: asset_name"


def test_platform_jobs_router_requires_server_project_ref_for_single_custom_code(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
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
        },
    )

    assert created.status_code == 400
    assert created.json()["detail"] == "platform job payload for single_generate/custom_code requires server_project_ref"


def test_platform_jobs_router_requires_server_project_ref_for_batch_custom_code(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
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
        },
    )

    assert created.status_code == 400
    assert created.json()["detail"] == "platform job payload for batch_generate/custom_code requires server_project_ref"


def test_platform_jobs_router_rate_limits_create_job_requests(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200
    profile_id = _seed_execution_profile(client)

    for index in range(5):
        created = client.post(
            "/api/platform/jobs",
            json={
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile_id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "card",
                        "input_summary": f"补一个卡牌实现方案-{index}",
                        "input_payload": {
                            "item_name": f"DarkBlade{index}",
                            "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                            "image_mode": "ai",
                        },
                    }
                ],
            },
        )
        assert created.status_code == 200

    limited = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
            "items": [
                {
                    "item_type": "card",
                    "input_summary": "补一个卡牌实现方案-limit",
                    "input_payload": {
                        "item_name": "DarkBladeLimit",
                        "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                        "image_mode": "ai",
                    },
                }
            ],
        },
    )

    assert limited.status_code == 429
    assert limited.json()["detail"] == "too many platform job create requests for user: limit 5 per 60 seconds"


def test_platform_jobs_router_rate_limits_start_job_requests(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)
    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        for index in range(6):
            job = JobRecord(
                user_id=user_id,
                job_type="single_generate",
                status=JobStatus.DRAFT,
                workflow_version="2026.03.31",
                input_summary=f"Job{index}",
                selected_execution_profile_id=profile_id,
                selected_agent_backend="codex",
                selected_model="gpt-5.4",
                total_item_count=1,
                pending_item_count=1,
                running_item_count=0,
                succeeded_item_count=0,
                failed_business_item_count=0,
                failed_system_item_count=0,
                quota_skipped_item_count=0,
                cancelled_before_start_item_count=0,
                cancelled_after_start_item_count=0,
                items=[
                    JobItemRecord(
                        user_id=user_id,
                        item_index=0,
                        item_type="card",
                        status=JobItemStatus.PENDING,
                        input_summary=f"Job{index}",
                        input_payload={
                            "item_name": f"DarkBlade{index}",
                            "description": "1 费攻击牌，造成 8 点伤害。",
                            "image_mode": "ai",
                        },
                    )
                ],
            )
            session.add(job)
        session.commit()
        job_ids = [
            row[0]
            for row in session.query(JobRecord.id)
            .filter(JobRecord.user_id == user_id)
            .order_by(JobRecord.id.asc())
            .all()
        ]
    finally:
        session.close()

    for job_id in job_ids[:5]:
        started = client.post(f"/api/platform/jobs/{job_id}/start", json={})
        assert started.status_code == 200

    limited = client.post(f"/api/platform/jobs/{job_ids[5]}/start", json={})

    assert limited.status_code == 429
    assert limited.json()["detail"] == "too many platform job start requests for user: limit 5 per 60 seconds"


def test_platform_jobs_router_accepts_server_project_ref(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    workspace_service = client.app.state.container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )

    assert created.status_code == 200
    assert created.json()["status"] == "draft"


def test_platform_jobs_router_returns_409_when_server_workspace_is_busy(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    workspace_service = client.app.state.container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")
    client.app.state.container.register_singleton(
        "platform.server_workspace_lock_service_factory",
        lambda: _BusyWorkspaceLockService(),
    )

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    started = client.post(f"/api/platform/jobs/{created.json()['id']}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "queued"

    detail = client.get(f"/api/platform/jobs/{created.json()['id']}")
    listed = client.get("/api/platform/jobs")
    events = client.get(f"/api/platform/jobs/{created.json()['id']}/events")

    assert detail.status_code == 200
    assert detail.json()["status"] == "queued"
    assert detail.json()["queued_reason_code"] == "server_workspace_busy"
    assert detail.json()["queued_reason_message"] == "server workspace is busy"
    assert listed.status_code == 200
    assert listed.json()[0]["status"] == "queued"
    assert listed.json()[0]["queued_reason_code"] == "server_workspace_busy"
    assert listed.json()[0]["queued_reason_message"] == "server workspace is busy"
    assert events.status_code == 200
    queued_events = [entry for entry in events.json() if entry["event_type"] == "job.queued"]
    assert queued_events[-1]["payload"]["reason_code"] == "server_workspace_busy"
    assert queued_events[-1]["payload"]["reason_message"] == "server workspace is busy"


def test_platform_jobs_router_can_complete_supported_log_analysis_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "log_analysis",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
            "items": [
                {
                    "item_type": "log_analysis",
                    "input_summary": "分析日志",
                    "input_payload": {"context": "黑屏了"},
                }
            ],
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "日志分析完成"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "日志分析完成"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_batch_custom_code_job(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已完成 BattleScriptManager 的服务器项目构建"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已完成 BattleScriptManager 的服务器项目构建"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_batch_card_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器卡牌实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器卡牌实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_batch_card_fullscreen_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"
    assert detail.json()["artifacts"] == []

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器全画面卡实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器全画面卡实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_batch_relic_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器遗物实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器遗物实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_batch_power_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器 Power 实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器 Power 实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_batch_character_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器角色实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器角色实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_single_custom_code_job(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    workspace_service = client.app.state.container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已完成 SingleEffectPatch 的服务器项目构建"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已完成 SingleEffectPatch 的服务器项目构建"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_single_relic_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器遗物实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器遗物实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_single_card_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器卡牌实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器卡牌实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_single_card_fullscreen_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器全画面卡实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器全画面卡实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_batch_card_fullscreen_with_uploaded_asset(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    workspace_service = client.app.state.container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")
    uploaded_asset_service = client.app.state.container.resolve_singleton("platform.uploaded_asset_service_factory")()
    uploaded = uploaded_asset_service.create_asset(
        user_id=user_id,
        file_name="fullscreen.png",
        content_base64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y3ioAAAAASUVORK5CYII=",
        mime_type="image/png",
    )
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["result_summary"] == "已完成 DarkBladeFullscreen 的服务器项目构建"


def test_platform_jobs_router_can_complete_single_card_fullscreen_with_uploaded_asset(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    workspace_service = client.app.state.container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")
    uploaded_asset_service = client.app.state.container.resolve_singleton("platform.uploaded_asset_service_factory")()
    uploaded = uploaded_asset_service.create_asset(
        user_id=user_id,
        file_name="fullscreen.png",
        content_base64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y3ioAAAAASUVORK5CYII=",
        mime_type="image/png",
    )
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["result_summary"] == "已完成 DarkBladeFullscreen 的服务器项目构建"


def test_platform_jobs_router_can_complete_supported_single_power_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器 Power 实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器 Power 实现方案"
    finally:
        session.close()


def test_platform_jobs_router_can_complete_supported_single_character_job(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")
    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile_id = _seed_execution_profile(client)
    client.app.state.container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/platform/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "selected_execution_profile_id": profile_id,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(f"/api/platform/jobs/{job_id}/start", json={})

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/platform/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/platform/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器角色实现方案"

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器角色实现方案"
    finally:
        session.close()
