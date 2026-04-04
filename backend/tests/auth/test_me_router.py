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
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import (
    JobItemRecord,
    JobRecord,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBucketRecord,
    QuotaBucketType,
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
            }
        },
        runtime_role="web",
    )
    session = container.resolve_singleton("auth.db_session_factory")()
    Base.metadata.create_all(session.bind)
    session.close()

    app = FastAPI()
    app.state.container = container
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
    assert detail.status_code == 200
    assert detail.json()["id"] == job.id
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
                    "input_payload": {"asset_name": "DarkRelic"},
                }
            ],
        },
    )
    assert created.status_code == 200
    assert created.json()["status"] == "draft"

    job_id = created.json()["id"]
    started = client.post(
        f"/api/me/jobs/{job_id}/start",
        json={"triggered_by": "user"},
    )

    assert started.status_code == 200
    assert started.json()["id"] == job_id
    assert started.json()["status"] == "queued"
