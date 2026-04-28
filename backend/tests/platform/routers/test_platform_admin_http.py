from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

pytest.importorskip("sqlalchemy")
fastapi = pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.platform.application.services.platform_runtime_audit_service import PlatformRuntimeAuditService
from app.modules.auth.application.services import PBKDF2PasswordHasher
from app.modules.platform.application.services.server_credential_health_checker import ServerCredentialHealthCheckResult
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.auth.infra.persistence.models import UserRecord
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    CredentialHealthCheckRecord,
    ExecutionChargeRecord,
    ExecutionProfileRecord,
    JobEventRecord,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBalanceRecord,
    ServerCredentialRecord,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.shared.infra.db.base import Base
from routers.auth_router import router as auth_router
from routers.platform_admin import router


class FakePlatformHealthChecker:
    def __init__(self, result: ServerCredentialHealthCheckResult | None = None) -> None:
        self.result = result or ServerCredentialHealthCheckResult(status="healthy", latency_ms=8)
        self.calls: list[dict] = []

    def check(self, **payload) -> ServerCredentialHealthCheckResult:
        self.calls.append(payload)
        return self.result


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "platform-admin.sqlite3"
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
    fake_health_checker = FakePlatformHealthChecker()
    container.register_singleton("platform.server_credential_health_checker_factory", lambda: fake_health_checker)
    runtime_audit_service = PlatformRuntimeAuditService(
        session_factory=container.resolve_singleton("platform.db_session_factory"),
        storage_root=tmp_path / "runtime-audit",
    )
    runtime_audit_service.append_event(
        event_type="runtime.queue_worker.leader_acquired",
        payload={
            "owner_id": "queue-worker:test",
            "leader_epoch": 1,
            "detail": "queue worker became leader",
        },
    )
    container.register_singleton("platform.runtime_audit_service_factory", runtime_audit_service)
    session = container.resolve_singleton("platform.db_session_factory")()
    cipher = ServerCredentialCipher.from_settings(container.resolve_singleton("settings"))
    Base.metadata.create_all(session.bind)
    job_repository = JobRepositorySqlAlchemy(session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.03.31", "items": [{"item_type": "card"}]}
        ),
    )
    session.flush()
    execution = AIExecutionRecord(
        job_id=job.id,
        job_item_id=job.items[0].id,
        user_id=1001,
        status="succeeded",
        provider="openai",
        model="gpt-5.4",
        credential_ref="cred-a",
        retry_attempt=1,
        switched_credential=True,
        request_idempotency_key="idem-admin",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-1",
    )
    session.add(execution)
    session.flush()
    session.add(
        ExecutionChargeRecord(
            ai_execution_id=execution.id,
            user_id=1001,
            charge_status="refunded",
            charge_amount=1,
            refund_reason="system_error",
        )
    )
    session.add(
        JobEventRecord(
            job_id=job.id,
            job_item_id=job.items[0].id,
            ai_execution_id=execution.id,
            user_id=1001,
            event_type="ai_execution.finished",
            event_payload={"status": "succeeded"},
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
            credential_ciphertext=cipher.encrypt("seed-openai-main"),
            secret_ciphertext=None,
            base_url="https://api.openai.com/v1",
            label="openai-main-a",
            priority=10,
            enabled=True,
            health_status="healthy",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )
    )
    session.add(
        UserRecord(
            username="admin",
            email="admin@example.com",
            password_hash=PBKDF2PasswordHasher(iterations=1).hash_password("admin-pass"),
            email_verified=True,
            is_admin=True,
        )
    )
    session.add(
        UserRecord(
            username="user",
            email="user@example.com",
            password_hash=PBKDF2PasswordHasher(iterations=1).hash_password("user-pass"),
            email_verified=True,
            is_admin=False,
        )
    )
    session.add(
        UserRecord(
            user_id=1001,
            username="quota-user",
            email="quota-user@example.com",
            password_hash=PBKDF2PasswordHasher(iterations=1).hash_password("quota-pass"),
            email_verified=True,
            is_admin=False,
        )
    )
    account = QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE)
    session.add(account)
    session.flush()
    session.add(
        QuotaBalanceRecord(
            user_id=1001,
            quota_account_id=account.id,
            total_limit=10,
            used_amount=2,
            refunded_amount=1,
            adjusted_amount=0,
            status=QuotaAccountStatus.ACTIVE,
        )
    )
    session.commit()
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(auth_router, prefix="/api")
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client, job.id, execution.id, fake_health_checker


def test_platform_admin_router_supports_execution_refund_and_audit_queries(client):
    test_client, job_id, execution_id, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    executions = test_client.get(f"/api/admin/jobs/{job_id}/executions")
    assert executions.status_code == 200
    assert executions.json()[0]["job_id"] == job_id

    detail = test_client.get(f"/api/admin/executions/{execution_id}")
    assert detail.status_code == 200
    assert detail.json()["request_idempotency_key"] == "idem-admin"
    assert detail.json()["credential_ref"] == "cred-a"
    assert detail.json()["retry_attempt"] == 1
    assert detail.json()["switched_credential"] is True

    refunds = test_client.get("/api/admin/quota/refunds", params={"user_id": 1001})
    assert refunds.status_code == 200
    assert refunds.json()[0]["refund_reason"] == "system_error"

    audit = test_client.get("/api/admin/audit/events", params={"job_id": job_id})
    assert audit.status_code == 200
    assert audit.json()[0]["event_type"] == "ai_execution.finished"

    merged_audit = test_client.get("/api/admin/audit/events")
    assert merged_audit.status_code == 200
    assert any(item["event_type"] == "runtime.queue_worker.leader_acquired" for item in merged_audit.json())

    filtered_runtime_audit = test_client.get(
        "/api/admin/audit/events",
        params={"event_type_prefix": "runtime.queue_worker."},
    )
    assert filtered_runtime_audit.status_code == 200
    assert filtered_runtime_audit.json()
    assert all(item["event_type"].startswith("runtime.queue_worker.") for item in filtered_runtime_audit.json())

    limited_runtime_audit = test_client.get(
        "/api/admin/audit/events",
        params={"event_type_prefix": "runtime.queue_worker.", "after_id": 0, "limit": 1},
    )
    assert limited_runtime_audit.status_code == 200
    assert len(limited_runtime_audit.json()) == 1

    credentials = test_client.get("/api/admin/platform/server-credentials")
    assert credentials.status_code == 200
    assert credentials.json()["items"][0]["label"] == "openai-main-a"
    assert credentials.json()["items"][0]["execution_profile_id"] == 1

    profiles = test_client.get("/api/admin/platform/execution-profiles")
    assert profiles.status_code == 200
    assert profiles.json()["items"][0]["code"] == "codex-gpt-5-4"


def test_platform_admin_router_supports_user_quota_management(client):
    test_client, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    users = test_client.get("/api/admin/users")
    assert users.status_code == 200
    assert any(item["user_id"] == 1001 for item in users.json()["items"])

    quota = test_client.get("/api/admin/users/1001/quota")
    assert quota.status_code == 200
    assert quota.json()["remaining"] == 9

    adjusted = test_client.post(
        "/api/admin/users/1001/quota/adjust",
        json={"direction": "grant", "amount": 5, "reason": "补偿测试额度"},
    )
    assert adjusted.status_code == 200
    assert adjusted.json()["adjusted_amount"] == 5
    assert adjusted.json()["remaining"] == 14

    ledger = test_client.get("/api/admin/users/1001/quota/ledger")
    assert ledger.status_code == 200
    assert ledger.json()["items"][0]["ledger_type"] == "admin_grant"

    rejected = test_client.post(
        "/api/admin/users/1001/quota/adjust",
        json={"direction": "deduct", "amount": 99, "reason": "too much"},
    )
    assert rejected.status_code == 400


def test_platform_admin_router_creates_server_credential_with_ciphertext_storage(client):
    test_client, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    created = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "provider": "openai",
            "auth_type": "api_key",
            "credential": "sk-live-main",
            "secret": "",
            "base_url": "https://api.openai.com/v1",
            "label": "openai-main-b",
            "priority": 20,
            "enabled": True,
        },
    )
    assert created.status_code == 200
    payload = created.json()
    assert payload["label"] == "openai-main-b"
    assert payload["provider"] == "openai"
    assert "credential" not in payload
    assert "secret" not in payload

    session = test_client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        row = session.query(ServerCredentialRecord).filter_by(label="openai-main-b").one()
        assert row.credential_ciphertext != "sk-live-main"
        assert row.secret_ciphertext is None
        cipher = ServerCredentialCipher.from_settings(test_client.app.state.container.resolve_singleton("settings"))
        assert cipher.decrypt(row.credential_ciphertext) == "sk-live-main"
    finally:
        session.close()


def test_platform_admin_router_updates_and_toggles_server_credential(client):
    test_client, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    updated = test_client.put(
        "/api/admin/platform/server-credentials/1",
        json={
            "execution_profile_id": 1,
            "provider": "anthropic",
            "auth_type": "api_key",
            "credential": "anthropic-key-1",
            "base_url": "https://api.anthropic.com",
            "label": "anthropic-main-a",
            "priority": 15,
            "enabled": True,
        },
    )
    assert updated.status_code == 200
    assert updated.json()["provider"] == "anthropic"
    assert updated.json()["label"] == "anthropic-main-a"

    disabled = test_client.post("/api/admin/platform/server-credentials/1/disable")
    assert disabled.status_code == 200
    assert disabled.json()["enabled"] is False
    assert disabled.json()["health_status"] == "disabled"

    enabled = test_client.post("/api/admin/platform/server-credentials/1/enable")
    assert enabled.status_code == 200
    assert enabled.json()["enabled"] is True
    assert enabled.json()["health_status"] == "degraded"


def test_platform_admin_router_runs_manual_health_check_and_writes_result(client):
    test_client, _, _, fake_health_checker = client

    fake_health_checker.result = ServerCredentialHealthCheckResult(
        status="rate_limited",
        error_code="http_429",
        error_message="limited",
        latency_ms=21,
    )
    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    checked = test_client.post("/api/admin/platform/server-credentials/1/health-check")
    assert checked.status_code == 200
    payload = checked.json()
    assert payload["credential_id"] == 1
    assert payload["health_status"] == "rate_limited"
    assert payload["error_code"] == "http_429"

    session = test_client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        row = session.query(ServerCredentialRecord).filter_by(id=1).one()
        assert row.health_status == "rate_limited"
        checks = session.query(CredentialHealthCheckRecord).filter_by(server_credential_id=1).all()
        assert len(checks) == 1
        assert checks[0].trigger_source == "manual"
        assert checks[0].status == "rate_limited"
    finally:
        session.close()


def test_platform_admin_router_requires_authenticated_admin_session(client):
    test_client, job_id, _, _ = client

    unauthenticated = test_client.get(f"/api/admin/jobs/{job_id}/executions")
    assert unauthenticated.status_code == 401

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "user@example.com",
            "password": "user-pass",
        },
    )
    assert login.status_code == 200

    forbidden = test_client.get(f"/api/admin/jobs/{job_id}/executions")
    assert forbidden.status_code == 403
    assert forbidden.json()["detail"] == "admin permission required"

    forbidden_write = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "provider": "openai",
            "auth_type": "api_key",
            "credential": "sk-denied",
            "label": "should-fail",
        },
    )
    assert forbidden_write.status_code == 403
    assert forbidden_write.json()["detail"] == "admin permission required"


def test_platform_admin_router_rejects_invalid_server_credential_payload(client):
    test_client, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    response = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "provider": "openai",
            "auth_type": "ak_sk",
            "credential": "ak-live",
            "label": "missing-secret",
        },
    )
    assert response.status_code == 400
    assert response.json()["detail"] == "secret is required when auth_type is ak_sk"

    missing = test_client.put(
        "/api/admin/platform/server-credentials/999",
        json={
            "execution_profile_id": 1,
            "provider": "openai",
            "auth_type": "api_key",
            "base_url": "https://api.openai.com/v1",
            "label": "missing",
            "priority": 1,
            "enabled": True,
        },
    )
    assert missing.status_code == 404
