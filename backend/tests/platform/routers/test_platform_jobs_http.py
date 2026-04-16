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
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionProfileRecord,
    QuotaAccountRecord,
    QuotaBucketRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)
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
        return [
            StepExecutionResult(
                step_id=steps[0].step_id,
                status="succeeded",
                output_payload={"text": "日志分析完成"},
                error_summary="",
            )
        ]


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
            "items": [{"item_type": "card", "input_payload": {"name": "DarkRelic"}}],
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
    assert "尚未为 single_generate/card 注册" in detail.json()["deferred_reason_message"]
    assert listed.status_code == 200
    assert listed.json()[0]["original_deducted"] == 1
    assert listed.json()[0]["refunded_amount"] == 1
    assert listed.json()[0]["net_consumed"] == 0
    assert listed.json()[0]["deferred_reason_code"] == "workflow_not_registered"
    assert "尚未为 single_generate/card 注册" in listed.json()[0]["deferred_reason_message"]

    items = client.get(f"/api/platform/jobs/{job_id}/items", params={"user_id": other_user_id})
    assert items.status_code == 200
    assert items.json()[0]["item_type"] == "card"
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
            "items": [{"item_type": "card", "input_payload": {"name": "DarkRelic"}}],
        },
    )

    assert created.status_code == 200
    assert created.json()["selected_execution_profile_id"] == profile_id
    assert created.json()["selected_agent_backend"] == "codex"
    assert created.json()["selected_model"] == "gpt-5.4"


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
