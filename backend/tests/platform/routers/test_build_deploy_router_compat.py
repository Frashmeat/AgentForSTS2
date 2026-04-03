from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

from fastapi import FastAPI
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from routers.build_deploy import router


class _FakeContainer:
    def __init__(self, *, service_split_enabled: bool, build_deploy_facade=None) -> None:
        self.platform_migration_flags = SimpleNamespace(
            platform_service_split_enabled=service_split_enabled,
            platform_runner_enabled=False,
        )
        self._singletons = {
            "platform.build_deploy_facade_service": build_deploy_facade,
        }

    def resolve_optional_singleton(self, key: str, default=None):
        return self._singletons.get(key, default)


def test_build_deploy_router_uses_facade_when_service_split_flag_enabled():
    calls: list[str] = []

    class FakeBuildDeployFacade:
        async def handle_ws_build_deploy(self, ws) -> None:
            calls.append("facade")
            await ws.accept()
            await ws.receive_text()
            await ws.send_text(json.dumps({"event": "done", "success": True, "source": "facade"}))

    app = FastAPI()
    app.state.container = _FakeContainer(
        service_split_enabled=True,
        build_deploy_facade=FakeBuildDeployFacade(),
    )
    app.include_router(router, prefix="/api")

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/build-deploy") as ws:
            ws.send_text(json.dumps({"project_root": "I:/not-used"}))
            payload = ws.receive_json()

    assert payload["source"] == "facade"
    assert calls == ["facade"]
