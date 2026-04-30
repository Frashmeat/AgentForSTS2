from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.modules.knowledge.infra import knowledge_runtime
from app.shared.infra.config.settings import Settings
from routers.workstation_capabilities import router


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
    monkeypatch.setattr(knowledge_runtime, "get_active_knowledge_pack", lambda: None)
    app = FastAPI()
    app.state.container = _FakeContainer(
        Settings.from_dict(
            {
                "platform_execution": {"control_token_env": "TEST_WORKSTATION_TOKEN"},
                "sts2_path": "",
            }
        )
    )
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def test_workstation_capabilities_requires_control_token(client: TestClient):
    response = client.get("/api/workstation/capabilities")

    assert response.status_code == 401


def test_workstation_capabilities_reports_linux_server_generation_boundary(client: TestClient):
    response = client.get(
        "/api/workstation/capabilities",
        headers={"X-ATS-Workstation-Token": "secret-token"},
    )

    assert response.status_code == 200
    payload = response.json()
    assert payload["knowledge"]["embedded_sts2_guidance"] is True
    assert payload["knowledge"]["knowledge_pack_active"] is False
    assert payload["knowledge"]["active_knowledge_pack_id"] == ""
    assert payload["knowledge"]["sts2_path_configured"] is False
    assert payload["generation"]["text_generation_available"] is True
    assert payload["generation"]["code_generation_available"] is True
    assert payload["build"]["server_build_supported"] is False
    assert payload["deploy"]["server_deploy_supported"] is False
