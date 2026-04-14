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
from app.modules.auth.application.services import PBKDF2PasswordHasher
from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.auth.infra.persistence.models import UserRecord
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionChargeRecord,
    ExecutionProfileRecord,
    JobEventRecord,
    ServerCredentialRecord,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.shared.infra.db.base import Base
from routers.auth_router import router as auth_router
from routers.platform_admin import router


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
    session = container.resolve_singleton("platform.db_session_factory")()
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
            credential_ciphertext="cipher",
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
    session.commit()
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(auth_router, prefix="/api")
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client, job.id, execution.id


def test_platform_admin_router_supports_execution_refund_and_audit_queries(client):
    test_client, job_id, execution_id = client

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

    credentials = test_client.get("/api/admin/platform/server-credentials")
    assert credentials.status_code == 200
    assert credentials.json()["items"][0]["label"] == "openai-main-a"
    assert credentials.json()["items"][0]["execution_profile_id"] == 1

    profiles = test_client.get("/api/admin/platform/execution-profiles")
    assert profiles.status_code == 200
    assert profiles.json()["items"][0]["code"] == "codex-gpt-5-4"


def test_platform_admin_router_requires_authenticated_admin_session(client):
    test_client, job_id, _ = client

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
