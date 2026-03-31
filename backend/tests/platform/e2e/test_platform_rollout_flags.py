from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

pytest.importorskip("sqlalchemy")
pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from routers.platform_admin import router as platform_admin_router
from routers.platform_jobs import router as platform_jobs_router


def _build_app(config: dict | None) -> tuple[FastAPI, ApplicationContainer]:
    container = ApplicationContainer.from_config(config)
    app = FastAPI()
    app.state.container = container

    if container.platform_migration_flags.platform_jobs_api_enabled:
        app.include_router(platform_jobs_router, prefix="/api")
    if container.platform_migration_flags.platform_service_split_enabled:
        app.include_router(platform_admin_router, prefix="/api")

    return app, container


def test_platform_rollout_mounts_follow_current_flags():
    app, _ = _build_app({"migration": {}})
    with TestClient(app) as client:
        assert client.get("/api/platform/jobs", params={"user_id": 1001}).status_code == 404
        assert client.get("/api/admin/quota/refunds").status_code == 404

    jobs_app, _ = _build_app({"migration": {"platform_jobs_api_enabled": True}})
    with TestClient(jobs_app) as client:
        assert client.get("/api/platform/jobs", params={"user_id": 1001}).status_code == 503
        assert client.get("/api/admin/quota/refunds").status_code == 404

    admin_app, _ = _build_app({"migration": {"platform_service_split_enabled": True}})
    with TestClient(admin_app) as client:
        assert client.get("/api/platform/jobs", params={"user_id": 1001}).status_code == 404
        assert client.get("/api/admin/quota/refunds").status_code == 503


def test_platform_rollout_runner_services_are_wired_when_database_and_flags_are_enabled():
    _, container = _build_app(
        {
            "database": {
                "url": "sqlite+pysqlite:///:memory:",
            },
            "migration": {
                "platform_runner_enabled": True,
                "platform_events_v1_enabled": True,
                "platform_step_protocol_enabled": True,
            },
        }
    )

    workflow_service = container.resolve_singleton("platform.workflow_router_compat_service")
    batch_service = container.resolve_singleton("platform.batch_workflow_router_compat_service")

    assert container.platform_migration_flags.platform_runner_enabled is True
    assert container.platform_migration_flags.platform_events_v1_enabled is True
    assert container.platform_migration_flags.platform_step_protocol_enabled is True
    assert workflow_service.session_factory is not None
    assert batch_service.session_factory is not None
