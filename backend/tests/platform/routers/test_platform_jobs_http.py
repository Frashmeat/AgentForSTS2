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
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBucketRecord
from app.shared.infra.db.base import Base
from routers.platform_jobs import router


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "platform-jobs.sqlite3"
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": f"sqlite+pysqlite:///{db_path.as_posix()}",
            },
            "migration": {
                "platform_jobs_api_enabled": True,
            },
        }
    )
    session = container.resolve_singleton("platform.db_session_factory")()
    Base.metadata.create_all(session.bind)
    session.add(QuotaAccountRecord(user_id=1001, status="active"))
    session.flush()
    session.add(
        QuotaBucketRecord(
            quota_account_id=1,
            bucket_type="daily",
            period_start=datetime.now(UTC) - timedelta(hours=1),
            period_end=datetime.now(UTC) + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )
    session.commit()
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def test_platform_jobs_router_supports_create_start_cancel_and_queries(client: TestClient):
    created = client.post(
        "/api/platform/jobs",
        params={"user_id": 1001},
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

    started = client.post(f"/api/platform/jobs/{job_id}/start", params={"user_id": 1001}, json={})
    assert started.status_code == 200
    assert started.json()["status"] == "queued"

    listed = client.get("/api/platform/jobs", params={"user_id": 1001})
    assert listed.status_code == 200
    assert listed.json()[0]["id"] == job_id

    detail = client.get(f"/api/platform/jobs/{job_id}", params={"user_id": 1001})
    assert detail.status_code == 200
    assert detail.json()["id"] == job_id

    items = client.get(f"/api/platform/jobs/{job_id}/items", params={"user_id": 1001})
    assert items.status_code == 200
    assert items.json()[0]["item_type"] == "card"

    events = client.get(f"/api/platform/jobs/{job_id}/events", params={"user_id": 1001})
    assert events.status_code == 200
    event_types = [entry["event_type"] for entry in events.json()]
    assert "job.created" in event_types
    assert "job.queued" in event_types

    quota = client.get("/api/platform/quota", params={"user_id": 1001})
    assert quota.status_code == 200
    assert quota.json()["daily_limit"] == 10

    cancelled = client.post(
        f"/api/platform/jobs/{job_id}/cancel",
        params={"user_id": 1001},
        json={"reason": "stop"},
    )
    assert cancelled.status_code == 200
    assert cancelled.json()["ok"] is True
