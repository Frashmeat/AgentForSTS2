from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

from fastapi import FastAPI
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

sys.modules.setdefault("image.generator", SimpleNamespace(generate_images=None))
sys.modules.setdefault("image.postprocess", SimpleNamespace(PROFILES={}, process_image=None))
sys.modules.setdefault("image.prompt_adapter", SimpleNamespace(adapt_prompt=None, ImageProvider=str))
sys.modules.setdefault("llm.text_runner", SimpleNamespace(complete_text=None))

import routers.workflow as workflow_router


class _FakeContainer:
    def __init__(self, *, runner_enabled: bool, workflow_router_service=None) -> None:
        self.platform_migration_flags = SimpleNamespace(
            platform_service_split_enabled=False,
            platform_runner_enabled=runner_enabled,
        )
        self._singletons = {
            "platform.workflow_router_compat_service": workflow_router_service,
        }

    def resolve_optional_singleton(self, key: str, default=None):
        return self._singletons.get(key, default)


def test_workflow_router_keeps_legacy_custom_code_path_when_runner_flag_disabled(monkeypatch):
    calls: list[str] = []

    async def fake_legacy_handler(ws, params, project_root):
        calls.append(f"legacy:{project_root}")
        await ws.send_text(json.dumps({"event": "done", "source": "legacy"}))

    monkeypatch.setattr(workflow_router, "_ws_run_custom_code", fake_legacy_handler)

    app = FastAPI()
    app.state.container = _FakeContainer(runner_enabled=False)
    app.include_router(workflow_router.router, prefix="/api")

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/create") as ws:
            ws.send_text(
                json.dumps(
                    {
                        "action": "start",
                        "asset_type": "custom_code",
                        "asset_name": "CompatCode",
                        "description": "legacy path",
                        "project_root": "I:/compat-project",
                    }
                )
            )
            payload = ws.receive_json()

    assert payload["source"] == "legacy"
    assert calls == ["legacy:I:\\compat-project"]


def test_workflow_router_uses_compat_service_when_runner_flag_enabled():
    calls: list[str] = []

    class FakeWorkflowRouterCompatService:
        async def handle_ws_create(self, ws) -> None:
            calls.append("compat")
            await ws.accept()
            await ws.receive_text()
            await ws.send_text(json.dumps({"event": "done", "source": "compat"}))

    app = FastAPI()
    app.state.container = _FakeContainer(
        runner_enabled=True,
        workflow_router_service=FakeWorkflowRouterCompatService(),
    )
    app.include_router(workflow_router.router, prefix="/api")

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/create") as ws:
            ws.send_text(
                json.dumps(
                    {
                        "action": "start",
                        "asset_type": "custom_code",
                        "asset_name": "CompatCode",
                        "description": "compat path",
                        "project_root": "I:/compat-project",
                    }
                )
            )
            payload = ws.receive_json()

    assert payload["source"] == "compat"
    assert calls == ["compat"]
