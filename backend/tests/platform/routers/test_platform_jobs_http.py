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
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBucketRecord
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


def test_platform_jobs_router_supports_create_start_cancel_and_queries_for_current_session_user(client: TestClient):
    current_user_id = _register_login_and_verify(client, "luna", "luna@example.com")
    other_user_id = _register_login_and_verify(client, "mira", "mira@example.com")
    assert other_user_id != current_user_id

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
            "items": [{"item_type": "card", "input_payload": {"name": "DarkRelic"}}],
        },
    )
    assert created.status_code == 200
    payload = created.json()
    job_id = payload["id"]
    assert payload["status"] == "draft"

    started = client.post(f"/api/platform/jobs/{job_id}/start", params={"user_id": other_user_id}, json={})
    assert started.status_code == 200
    assert started.json()["status"] == "queued"

    listed = client.get("/api/platform/jobs", params={"user_id": other_user_id})
    assert listed.status_code == 200
    assert listed.json()[0]["id"] == job_id

    detail = client.get(f"/api/platform/jobs/{job_id}", params={"user_id": other_user_id})
    assert detail.status_code == 200
    assert detail.json()["id"] == job_id

    items = client.get(f"/api/platform/jobs/{job_id}/items", params={"user_id": other_user_id})
    assert items.status_code == 200
    assert items.json()[0]["item_type"] == "card"

    events = client.get(f"/api/platform/jobs/{job_id}/events", params={"user_id": other_user_id})
    assert events.status_code == 200
    event_types = [entry["event_type"] for entry in events.json()]
    assert "job.created" in event_types
    assert "job.queued" in event_types

    quota = client.get("/api/platform/quota", params={"user_id": other_user_id})
    assert quota.status_code == 200
    assert quota.json()["daily_limit"] == 10

    cancelled = client.post(
        f"/api/platform/jobs/{job_id}/cancel",
        params={"user_id": other_user_id},
        json={"reason": "stop"},
    )
    assert cancelled.status_code == 200
    assert cancelled.json()["ok"] is True
