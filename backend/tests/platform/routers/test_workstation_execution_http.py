from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.shared.infra.config.settings import Settings
from routers import WORKSTATION_ROUTER_MODULES
from routers.workstation_platform import router
from app.modules.platform.contracts.workstation_execution import WorkstationExecutionEvent, WorkstationExecutionPollResult


class _FakeWorkstationExecutor:
    def execute(self, request, event_sink=None):
        if event_sink is not None:
            event_sink(
                WorkstationExecutionEvent(
                    sequence=1,
                    event_type="workstation.step.started",
                    occurred_at="2026-04-29T10:00:00+00:00",
                    payload={"step_id": "single.relic.plan", "step_type": "single.asset.plan"},
                )
            )
        return WorkstationExecutionPollResult(
            workstation_execution_id=f"ws-exec-{request.execution_id}",
            status="succeeded",
            step_id="single.relic.plan",
            output_payload={"text": "ok"},
        )


class _FakeContainer:
    def __init__(self, settings: Settings) -> None:
        self._settings = settings

    def resolve_optional_singleton(self, key: str, default=None):
        if key == "settings":
            return self._settings
        return default


@pytest.fixture()
def client(monkeypatch):
    monkeypatch.setenv("TEST_WORKSTATION_TOKEN", "secret-token")
    app = FastAPI()
    app.state.container = _FakeContainer(
        Settings.from_dict(
            {
                "platform_execution": {
                    "control_token_env": "TEST_WORKSTATION_TOKEN",
                }
            }
        )
    )
    app.state.workstation_platform_executor = _FakeWorkstationExecutor()
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def _dispatch_payload() -> dict:
    return {
        "execution_id": 2203,
        "job_id": 2002,
        "job_item_id": 2103,
        "job_type": "single_generate",
        "item_type": "relic",
        "workflow_version": "2026.03.31",
        "step_protocol_version": "v1",
        "result_schema_version": "v1",
        "input_payload": {
            "item_name": "FangedGrimoire",
            "description": "每次造成伤害时获得 2 点格挡。",
        },
        "execution_binding": {
            "agent_backend": "codex",
            "provider": "openai",
            "model": "gpt-5.4",
            "credential_ref": "server-credential:1",
            "auth_type": "api_key",
            "credential": "sk-live",
            "base_url": "https://api.openai.com/v1",
        },
    }


def test_workstation_platform_router_is_registered_for_workstation_role():
    assert "routers.workstation_platform" in WORKSTATION_ROUTER_MODULES
    assert "routers.workstation_capabilities" in WORKSTATION_ROUTER_MODULES


def test_workstation_execution_dispatch_requires_control_token(client: TestClient):
    response = client.post("/api/workstation/platform/executions", json=_dispatch_payload())

    assert response.status_code == 401
    assert response.json()["detail"] == "workstation control token required"


def test_workstation_execution_dispatch_rejects_wrong_control_token(client: TestClient):
    response = client.post(
        "/api/workstation/platform/executions",
        json=_dispatch_payload(),
        headers={"X-ATS-Workstation-Token": "wrong"},
    )

    assert response.status_code == 403
    assert response.json()["detail"] == "invalid workstation control token"


def test_workstation_execution_dispatch_accepts_and_poll_hides_credentials(client: TestClient):
    accepted = client.post(
        "/api/workstation/platform/executions",
        json=_dispatch_payload(),
        headers={"X-ATS-Workstation-Token": "secret-token"},
    )

    assert accepted.status_code == 200
    accepted_payload = accepted.json()
    assert accepted_payload == {
        "workstation_execution_id": "ws-exec-2203",
        "status": "accepted",
        "poll_url": "/api/workstation/platform/executions/ws-exec-2203",
    }

    polled = client.get(
        accepted_payload["poll_url"],
        headers={"X-ATS-Workstation-Token": "secret-token"},
    )

    assert polled.status_code == 200
    poll_payload = polled.json()
    assert poll_payload["workstation_execution_id"] == "ws-exec-2203"
    assert poll_payload["status"] in {"accepted", "running", "succeeded"}
    if poll_payload["status"] == "succeeded":
        assert poll_payload["events"][0]["event_type"] == "workstation.step.started"
    assert "credential" not in str(poll_payload)


def test_workstation_execution_dispatch_fails_closed_when_env_token_missing(monkeypatch):
    monkeypatch.delenv("TEST_WORKSTATION_TOKEN", raising=False)
    app = FastAPI()
    app.state.container = _FakeContainer(
        Settings.from_dict(
            {
                "platform_execution": {
                    "control_token_env": "TEST_WORKSTATION_TOKEN",
                }
            }
        )
    )
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        response = test_client.post(
            "/api/workstation/platform/executions",
            json=_dispatch_payload(),
            headers={"X-ATS-Workstation-Token": "secret-token"},
        )

    assert response.status_code == 503
    assert response.json()["detail"] == "workstation control token is not configured"
