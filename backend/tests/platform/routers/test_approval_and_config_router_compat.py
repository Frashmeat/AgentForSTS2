from __future__ import annotations

import sys
from pathlib import Path
from types import SimpleNamespace

from fastapi import FastAPI
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from approval.models import ActionRequest
from approval.runtime import get_approval_store, reset_approval_runtime
from routers.approval_router import router as approval_router
from routers.config_router import router as config_router


class _FakeContainer:
    def __init__(self, *, service_split_enabled: bool, approval_facade=None, config_facade=None) -> None:
        self.platform_migration_flags = SimpleNamespace(
            platform_service_split_enabled=service_split_enabled,
            platform_runner_enabled=False,
        )
        self._singletons = {
            "platform.approval_facade_service": approval_facade,
            "platform.config_facade_service": config_facade,
        }

    def resolve_optional_singleton(self, key: str, default=None):
        return self._singletons.get(key, default)


def _create_action() -> ActionRequest:
    return ActionRequest(
        kind="write_file",
        title="Write source",
        reason="Need generated file",
        payload={"path": "Cards/TestCard.cs"},
        risk_level="medium",
        requires_approval=True,
        source_backend="codex",
        source_workflow="single_asset",
    )


def test_approval_router_uses_facade_when_service_split_flag_enabled():
    calls: list[tuple[str, str]] = []

    class FakeApprovalFacade:
        def list_requests(self):
            calls.append(("list", "all"))
            return [{"action_id": "facade-1", "status": "approved"}]

    app = FastAPI()
    app.state.container = _FakeContainer(
        service_split_enabled=True,
        approval_facade=FakeApprovalFacade(),
    )
    app.include_router(approval_router, prefix="/api")

    with TestClient(app) as client:
        response = client.get("/api/approvals")

    assert response.status_code == 200
    assert response.json() == [{"action_id": "facade-1", "status": "approved"}]
    assert calls == [("list", "all")]


def test_config_router_uses_facade_when_service_split_flag_enabled():
    calls: list[str] = []

    class FakeConfigFacade:
        def get_masked_config(self):
            calls.append("get")
            return {"llm": {"api_key": "****1234"}}

    app = FastAPI()
    app.state.container = _FakeContainer(
        service_split_enabled=True,
        config_facade=FakeConfigFacade(),
    )
    app.include_router(config_router, prefix="/api")

    with TestClient(app) as client:
        response = client.get("/api/config")

    assert response.status_code == 200
    assert response.json() == {"llm": {"api_key": "****1234"}}
    assert calls == ["get"]


def test_approval_router_keeps_legacy_behavior_when_service_split_flag_disabled():
    reset_approval_runtime()
    store = get_approval_store()
    action = store.create_request(_create_action())

    app = FastAPI()
    app.state.container = _FakeContainer(service_split_enabled=False)
    app.include_router(approval_router, prefix="/api")

    with TestClient(app) as client:
        response = client.get(f"/api/approvals/{action.action_id}")

    assert response.status_code == 200
    assert response.json()["action_id"] == action.action_id
