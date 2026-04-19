from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

pytest.importorskip("sqlalchemy")
pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService
from app.modules.platform.application.services.uploaded_asset_service import UploadedAssetService
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionProfileRecord,
    JobEventRecord,
    JobItemRecord,
    JobRecord,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBucketRecord,
    QuotaBucketType,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.shared.infra.db.base import Base
from routers.auth_router import router as auth_router
from routers.me_router import router as me_router


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "me-router.sqlite3"
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
    session = container.resolve_singleton("auth.db_session_factory")()
    Base.metadata.create_all(session.bind)
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
    app.include_router(me_router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def test_me_router_returns_current_user_profile(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    profile = client.get("/api/me/profile")
    assert profile.status_code == 200
    assert profile.json()["username"] == "luna"
    assert profile.json()["email"] == "luna@example.com"


def test_me_router_can_upload_server_asset(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    uploaded = client.post(
        "/api/me/upload-assets",
        json={
            "file_name": "dark-blade.png",
            "content_base64": "ZmFrZS1pbWFnZS1ieXRlcw==",
            "mime_type": "image/png",
        },
    )

    assert uploaded.status_code == 200
    assert uploaded.json()["uploaded_asset_ref"].startswith("uploaded-asset:")
    assert uploaded.json()["file_name"] == "dark-blade.png"
    assert uploaded.json()["mime_type"] == "image/png"


def test_me_router_can_create_server_workspace(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    workspace = client.post(
        "/api/me/server-workspaces",
        json={"project_name": "DarkMod"},
    )

    assert workspace.status_code == 200
    assert workspace.json()["server_project_ref"].startswith("server-workspace:")
    assert workspace.json()["project_name"] == "DarkMod"


def test_me_router_can_create_platform_job_with_server_project_ref(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    workspace_service = app_container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")

    session = app_container.resolve_singleton("platform.db_session_factory")()
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
                credential_ciphertext="cipher",
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
        session.add(QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE))
        session.commit()
    finally:
        session.close()

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个单资产脚本",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
        },
    )

    assert created.status_code == 200
    assert created.json()["status"] == "draft"


def test_me_router_returns_zero_quota_view_for_new_user_without_quota_records(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    quota = client.get("/api/me/quota")

    assert quota.status_code == 200
    assert quota.json()["daily_limit"] == 0
    assert quota.json()["daily_used"] == 0
    assert quota.json()["weekly_limit"] == 0
    assert quota.json()["weekly_used"] == 0
    assert quota.json()["refunded"] == 0


def test_me_router_exposes_quota_and_platform_jobs(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    app_container = client.app.state.container
    session = app_container.resolve_singleton("platform.db_session_factory")()
    now = datetime.now(UTC)
    account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
    session.add(account)
    session.flush()
    session.add(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=2,
            refunded_amount=1,
        )
    )
    job = JobRecord(
        user_id=user_id,
        job_type="single_generate",
        status=JobStatus.RUNNING,
        workflow_version="2026.04.03",
        input_summary="Dark Relic",
        total_item_count=1,
        pending_item_count=0,
        running_item_count=1,
        succeeded_item_count=0,
        failed_business_item_count=0,
        failed_system_item_count=0,
        quota_skipped_item_count=0,
        cancelled_before_start_item_count=0,
        cancelled_after_start_item_count=0,
    )
    session.add(job)
    session.flush()
    session.add(
        JobItemRecord(
            job_id=job.id,
            user_id=user_id,
            item_index=0,
            item_type="card",
            status=JobItemStatus.RUNNING,
            input_summary="Dark Relic",
        )
    )
    session.commit()
    session.close()

    quota = client.get("/api/me/quota")
    jobs = client.get("/api/me/jobs")
    detail = client.get(f"/api/me/jobs/{job.id}")
    items = client.get(f"/api/me/jobs/{job.id}/items")

    assert quota.status_code == 200
    assert quota.json()["daily_limit"] == 10
    assert jobs.status_code == 200
    assert jobs.json()[0]["job_type"] == "single_generate"
    assert jobs.json()[0]["deferred_reason_code"] == ""
    assert detail.status_code == 200
    assert detail.json()["id"] == job.id
    assert detail.json()["deferred_reason_code"] == ""
    assert items.status_code == 200
    assert items.json()[0]["item_type"] == "card"


def test_me_router_can_create_and_start_current_user_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=registered.json()["user"]["user_id"], status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.04.04",
            "input_summary": "Dark Relic",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
            "selected_agent_backend": "codex",
            "selected_model": "gpt-5.4",
            "items": [
                {
                    "item_type": "potion",
                    "input_summary": "Dark Relic",
                    "input_payload": {
                        "item_name": "DarkPotion",
                        "description": "造成 8 点伤害。",
                    },
                }
            ],
        },
    )
    assert created.status_code == 200
    assert created.json()["status"] == "draft"
    assert created.json()["selected_execution_profile_id"] == 1
    assert created.json()["selected_agent_backend"] == "codex"
    assert created.json()["selected_model"] == "gpt-5.4"

    job_id = created.json()["id"]
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["id"] == job_id
    assert started.json()["status"] == "deferred"

    detail = client.get(f"/api/me/jobs/{job_id}")
    jobs = client.get("/api/me/jobs")
    assert detail.status_code == 200
    assert detail.json()["status"] == "deferred"
    assert detail.json()["selected_execution_profile_id"] == 1
    assert detail.json()["selected_agent_backend"] == "codex"
    assert detail.json()["selected_model"] == "gpt-5.4"
    assert detail.json()["original_deducted"] == 1
    assert detail.json()["refunded_amount"] == 1
    assert detail.json()["net_consumed"] == 0
    assert detail.json()["deferred_reason_code"] == "workflow_not_registered"
    assert "尚未为 single_generate/potion 注册" in detail.json()["deferred_reason_message"]
    assert jobs.status_code == 200
    assert jobs.json()[0]["original_deducted"] == 1
    assert jobs.json()[0]["refunded_amount"] == 1
    assert jobs.json()[0]["net_consumed"] == 0
    assert jobs.json()[0]["deferred_reason_code"] == "workflow_not_registered"
    assert "尚未为 single_generate/potion 注册" in jobs.json()[0]["deferred_reason_message"]

    quota = client.get("/api/me/quota")
    assert quota.status_code == 200
    assert quota.json()["refunded"] == 1

    items = client.get(f"/api/me/jobs/{job_id}/items")
    events = client.get(f"/api/me/jobs/{job_id}/events")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "deferred"
    assert events.status_code == 200
    event_types = [entry["event_type"] for entry in events.json()]
    assert "ai_execution.deferred" in event_types
    assert "ai_execution.finished" in event_types
    deferred_event = next(entry for entry in events.json() if entry["event_type"] == "ai_execution.deferred")
    assert deferred_event["payload"]["reason_code"] == "workflow_not_registered"
    assert "ai_execution_id" not in deferred_event
    finished_event = next(entry for entry in events.json() if entry["event_type"] == "ai_execution.finished")
    assert finished_event["payload"]["status"] == "completed_with_refund"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "completed_with_refund"
        assert execution.provider == "openai"
        assert execution.model == "gpt-5.4"
        assert execution.credential_ref == "server-credential:1"
    finally:
        session.close()


def test_me_router_create_job_uses_current_user_default_server_profile_when_request_omits_selection(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    user_id = registered.json()["user"]["user_id"]
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    session = app_container.resolve_singleton("platform.db_session_factory")()
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
                credential_ciphertext="cipher",
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
        session.add(
            UserPlatformPreferenceRecord(
                user_id=user_id,
                default_execution_profile_id=profile.id,
            )
        )
        session.commit()
    finally:
        session.close()

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.04.04",
            "input_summary": "Dark Relic",
            "created_from": "single_asset",
            "items": [
                {
                    "item_type": "relic",
                    "input_summary": "Dark Relic",
                    "input_payload": {"item_name": "DarkRelic"},
                }
            ],
        },
    )

    assert created.status_code == 200
    assert created.json()["selected_execution_profile_id"] == 1
    assert created.json()["selected_agent_backend"] == "codex"
    assert created.json()["selected_model"] == "gpt-5.4"


def test_me_router_rejects_legacy_platform_payload_fields(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    session = app_container.resolve_singleton("platform.db_session_factory")()
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
                credential_ciphertext="cipher",
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
        session.add(QuotaAccountRecord(user_id=registered.json()["user"]["user_id"], status=QuotaAccountStatus.ACTIVE))
        session.commit()
    finally:
        session.close()

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个卡牌实现方案",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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


def test_me_router_requires_server_project_ref_for_single_custom_code(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    session = app_container.resolve_singleton("platform.db_session_factory")()
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
                credential_ciphertext="cipher",
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
        session.add(QuotaAccountRecord(user_id=registered.json()["user"]["user_id"], status=QuotaAccountStatus.ACTIVE))
        session.commit()
    finally:
        session.close()

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个单资产脚本",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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


def test_me_router_requires_server_project_ref_for_batch_custom_code(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    session = app_container.resolve_singleton("platform.db_session_factory")()
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
                credential_ciphertext="cipher",
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
        session.add(QuotaAccountRecord(user_id=registered.json()["user"]["user_id"], status=QuotaAccountStatus.ACTIVE))
        session.commit()
    finally:
        session.close()

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个战斗脚本管理器",
            "created_from": "batch_generation",
            "selected_execution_profile_id": 1,
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


def test_me_router_rejects_platform_job_actions_for_unverified_email(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.04.04",
            "input_summary": "Dark Relic",
            "created_from": "single_asset",
            "items": [
                {
                    "item_type": "relic",
                    "input_summary": "Dark Relic",
                    "input_payload": {"item_name": "DarkRelic"},
                }
            ],
        },
    )
    assert created.status_code == 403
    assert created.json()["detail"] == "email verification required"


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
                output_payload = {"text": "已写入 SingleEffectPatch 的服务器 custom_code 代码"}
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


def test_me_router_can_complete_supported_log_analysis_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "log_analysis",
            "workflow_version": "2026.03.31",
            "input_summary": "分析日志",
            "created_from": "log_analysis",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "日志分析完成"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "日志分析完成"
    finally:
        session.close()


def test_me_router_can_complete_supported_batch_card_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个批量卡牌实现方案",
            "created_from": "batch_generation",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器卡牌实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器卡牌实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_batch_card_fullscreen_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个批量全画面卡实现方案",
            "created_from": "batch_generation",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器全画面卡实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器全画面卡实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_batch_relic_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个批量遗物实现方案",
            "created_from": "batch_generation",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器遗物实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器遗物实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_batch_power_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个批量 Power 实现方案",
            "created_from": "batch_generation",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器 Power 实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器 Power 实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_batch_character_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个批量角色实现方案",
            "created_from": "batch_generation",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器角色实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器角色实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_single_custom_code_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    workspace_service = app_container.resolve_singleton("platform.server_workspace_service_factory")()
    workspace = workspace_service.create_workspace(user_id=user_id, project_name="DarkMod")
    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个单资产脚本",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已写入 SingleEffectPatch 的服务器 custom_code 代码"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已写入 SingleEffectPatch 的服务器 custom_code 代码"
    finally:
        session.close()


def test_me_router_can_complete_supported_single_relic_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个遗物实现方案",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器遗物实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器遗物实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_single_card_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个卡牌实现方案",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器卡牌实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器卡牌实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_single_card_fullscreen_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个全画面卡实现方案",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
        },
    )
    assert created.status_code == 200

    job_id = created.json()["id"]
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器全画面卡实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器全画面卡实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_single_power_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个 Power 实现方案",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器 Power 实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器 Power 实现方案"
    finally:
        session.close()


def test_me_router_can_complete_supported_single_character_job(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200

    app_container = client.app.state.container
    cipher = ServerCredentialCipher.from_settings(app_container.resolve_singleton("settings"))
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        account = QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
        session.add(account)
        session.flush()
        session.add(
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
    finally:
        session.close()

    app_container.register_singleton("platform.workflow_runner_factory", _SucceededWorkflowRunner)

    created = client.post(
        "/api/me/jobs",
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "补一个角色实现方案",
            "created_from": "single_asset",
            "selected_execution_profile_id": 1,
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
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["status"] == "succeeded"

    detail = client.get(f"/api/me/jobs/{job_id}")
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"

    items = client.get(f"/api/me/jobs/{job_id}/items")
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert items.json()[0]["result_summary"] == "已生成服务器角色实现方案"

    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        execution = session.query(AIExecutionRecord).filter(AIExecutionRecord.job_id == job_id).one()
        assert execution.status.value == "succeeded"
        assert execution.result_summary == "已生成服务器角色实现方案"
    finally:
        session.close()


def test_me_router_job_summary_uses_latest_deferred_event(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    user_id = registered.json()["user"]["user_id"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    app_container = client.app.state.container
    session = app_container.resolve_singleton("platform.db_session_factory")()
    try:
        job = JobRecord(
            user_id=user_id,
            job_type="single_generate",
            status=JobStatus.RUNNING,
            workflow_version="2026.04.03",
            input_summary="Dark Relic",
            total_item_count=1,
            pending_item_count=0,
            running_item_count=1,
            succeeded_item_count=0,
            failed_business_item_count=0,
            failed_system_item_count=0,
            quota_skipped_item_count=0,
            cancelled_before_start_item_count=0,
            cancelled_after_start_item_count=0,
        )
        session.add(job)
        session.flush()
        session.add(
            JobEventRecord(
                job_id=job.id,
                user_id=user_id,
                event_type="ai_execution.deferred",
                event_payload={
                    "reason_code": "workflow_not_registered",
                    "reason_message": "older",
                },
            )
        )
        session.add(
            JobEventRecord(
                job_id=job.id,
                user_id=user_id,
                event_type="ai_execution.deferred",
                event_payload={
                    "reason_code": "local_project_root_required",
                    "reason_message": "latest",
                },
            )
        )
        session.commit()
    finally:
        session.close()

    jobs = client.get("/api/me/jobs")
    assert jobs.status_code == 200
    assert jobs.json()[0]["deferred_reason_code"] == "local_project_root_required"
    assert jobs.json()[0]["deferred_reason_message"] == "latest"
