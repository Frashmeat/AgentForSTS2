from __future__ import annotations

import json
import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

sys.modules.setdefault("image.generator", SimpleNamespace(generate_images=None))
sys.modules.setdefault("image.postprocess", SimpleNamespace(PROFILES={}, process_image=None))
sys.modules.setdefault("image.prompt_adapter", SimpleNamespace(adapt_prompt=None, ImageProvider=str))
sys.modules.setdefault("llm.text_runner", SimpleNamespace(complete_text=None))

pytest.importorskip("sqlalchemy")
pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.platform.application.services.workflow_router_compat_service import WorkflowRouterCompatService
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBucketRecord
from app.shared.infra.db.base import Base
from routers.platform_jobs import router as platform_jobs_router
from routers.workflow import router as workflow_router


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "platform-lifecycle.sqlite3"
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": f"sqlite+pysqlite:///{db_path.as_posix()}",
            },
            "migration": {
                "platform_jobs_api_enabled": True,
                "platform_runner_enabled": True,
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

    service = container.resolve_singleton("platform.workflow_router_compat_service")
    assert isinstance(service, WorkflowRouterCompatService)

    async def fake_create_custom_code(**kwargs):
        await kwargs["stream_callback"]("lifecycle chunk")
        return f"generated {kwargs['name']}"

    service.create_custom_code_fn = fake_create_custom_code

    app = FastAPI()
    app.state.container = container
    app.include_router(workflow_router, prefix="/api")
    app.include_router(platform_jobs_router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def test_platform_job_lifecycle_runner_path_is_queryable_via_platform_api(client: TestClient):
    with client.websocket_connect("/api/ws/create") as ws:
        ws.send_text(
            json.dumps(
                {
                    "action": "start",
                    "asset_type": "custom_code",
                    "asset_name": "LifecycleCode",
                    "description": "通过平台主链执行最小生命周期",
                    "implementation_notes": "写一个生命周期测试桩",
                    "project_root": "I:/lifecycle-project",
                    "user_id": 1001,
                }
            )
        )

        events: list[dict] = []
        while True:
            payload = ws.receive_json()
            events.append(payload)
            if payload.get("event") == "done":
                break

    done = events[-1]
    job_id = done["job_id"]

    detail = client.get(f"/api/platform/jobs/{job_id}", params={"user_id": 1001})
    items = client.get(f"/api/platform/jobs/{job_id}/items", params={"user_id": 1001})
    job_events = client.get(f"/api/platform/jobs/{job_id}/events", params={"user_id": 1001})
    quota = client.get("/api/platform/quota", params={"user_id": 1001})

    assert done["success"] is True
    assert any(entry["event"] == "agent_stream" for entry in events)
    assert detail.status_code == 200
    assert detail.json()["status"] == "succeeded"
    assert items.status_code == 200
    assert items.json()[0]["status"] == "succeeded"
    assert [entry["event_type"] for entry in job_events.json()] == [
        "job.created",
        "job.queued",
        "job.item.completed",
        "job.completed",
    ]
    assert quota.status_code == 200
    assert quota.json()["daily_limit"] == 10


def test_platform_job_lifecycle_cancel_flow_is_visible_in_platform_events(client: TestClient):
    created = client.post(
        "/api/platform/jobs",
        params={"user_id": 1001},
        json={
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "items": [{"item_type": "custom_code", "input_payload": {"name": "CancelFlow"}}],
        },
    )
    job_id = created.json()["id"]

    started = client.post(
        f"/api/platform/jobs/{job_id}/start",
        params={"user_id": 1001},
        json={"triggered_by": "task11-test"},
    )
    cancelled = client.post(
        f"/api/platform/jobs/{job_id}/cancel",
        params={"user_id": 1001},
        json={"reason": "task11 cancel"},
    )
    job_events = client.get(f"/api/platform/jobs/{job_id}/events", params={"user_id": 1001})

    assert created.status_code == 200
    assert started.status_code == 200
    assert cancelled.status_code == 200
    assert cancelled.json()["ok"] is True
    assert [entry["event_type"] for entry in job_events.json()] == [
        "job.created",
        "job.queued",
        "job.cancel_requested",
    ]
