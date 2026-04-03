from __future__ import annotations

import json
import sys
from pathlib import Path

from fastapi import FastAPI
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import routers.build_deploy as build_deploy_router


def test_build_deploy_ws_returns_error_when_sts2_path_is_configured_but_missing(monkeypatch, tmp_path):
    async def fake_build_and_fix(project_root, stream_callback=None):
        if stream_callback is not None:
            await stream_callback("build ok")
        return True, ""

    monkeypatch.setattr(build_deploy_router, "build_and_fix", fake_build_and_fix)
    monkeypatch.setattr(
        build_deploy_router,
        "get_config",
        lambda: {"sts2_path": str(tmp_path / "missing-sts2-root")},
    )

    app = FastAPI()
    app.include_router(build_deploy_router.router, prefix="/api")
    project_root = tmp_path / "MyMod"
    project_root.mkdir()

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/build-deploy") as ws:
            ws.send_text(json.dumps({"project_root": str(project_root)}))
            payload = ws.receive_json()

    assert payload["event"] == "error"
