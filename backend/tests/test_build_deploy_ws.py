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

    expected_message = build_deploy_router._TEXT_LOADER.render(
        "runtime_workflow.build_game_path_invalid",
        {"target_dir": tmp_path / "missing-sts2-root" / "Mods"},
    ).strip()
    assert payload == {
        "event": "error",
        "code": "sts2_mods_path_invalid",
        "message": expected_message,
        "detail": expected_message,
    }


def test_build_deploy_ws_stream_includes_model_metadata(monkeypatch, tmp_path):
    async def fake_build_and_fix(project_root, stream_callback=None):
        if stream_callback is not None:
            await stream_callback("build ok")
        return True, ""

    game_root = tmp_path / "Game"
    (game_root / "Mods").mkdir(parents=True)

    monkeypatch.setattr(build_deploy_router, "build_and_fix", fake_build_and_fix)
    monkeypatch.setattr(
        build_deploy_router,
        "get_config",
        lambda: {
            "sts2_path": str(game_root),
            "llm": {
                "agent_backend": "codex",
                "model": "gpt-5.4",
            },
        },
    )

    app = FastAPI()
    app.include_router(build_deploy_router.router, prefix="/api")
    project_root = tmp_path / "MyMod"
    project_root.mkdir()

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/build-deploy") as ws:
            ws.send_text(json.dumps({"project_root": str(project_root)}))
            payloads = []
            while True:
                payload = ws.receive_json()
                payloads.append(payload)
                if payload.get("event") == "done":
                    break

    first_stream, second_stream, third_stream, *rest = payloads
    done_payload = rest[-1]

    assert first_stream == {
        "event": "stream",
        "chunk": build_deploy_router._TEXT_LOADER.load("runtime_workflow.build_agent_build_start").strip() + "\n",
        "source": "build",
        "channel": "raw",
        "model": "gpt-5.4",
    }
    assert second_stream == {
        "event": "stream",
        "chunk": "build ok",
        "source": "build",
        "channel": "raw",
        "model": "gpt-5.4",
    }
    assert third_stream == {
        "event": "stream",
        "chunk": "\n" + build_deploy_router._TEXT_LOADER.load("runtime_workflow.build_build_succeeded").strip() + "\n",
        "source": "build",
        "channel": "raw",
        "model": "gpt-5.4",
    }
    assert done_payload["event"] == "done"


def test_build_deploy_ws_syncs_local_props_before_build(monkeypatch, tmp_path):
    called: list[Path] = []

    async def fake_build_and_fix(project_root, stream_callback=None):
        return True, ""

    monkeypatch.setattr(build_deploy_router, "build_and_fix", fake_build_and_fix)
    monkeypatch.setattr(build_deploy_router, "ensure_local_props", lambda project_root: called.append(project_root) or True)
    monkeypatch.setattr(
        build_deploy_router,
        "get_config",
        lambda: {"sts2_path": ""},
    )

    app = FastAPI()
    app.include_router(build_deploy_router.router, prefix="/api")
    project_root = tmp_path / "MyMod"
    project_root.mkdir()

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/build-deploy") as ws:
            ws.send_text(json.dumps({"project_root": str(project_root)}))
            while True:
                payload = ws.receive_json()
                if payload.get("event") == "done":
                    break

    assert called == [project_root]


def test_build_deploy_ws_writes_deploy_registry_metadata(monkeypatch, tmp_path):
    async def fake_build_and_fix(project_root, stream_callback=None):
        if stream_callback is not None:
            await stream_callback("build ok")
        return True, ""

    game_root = tmp_path / "Game"
    (game_root / "Mods").mkdir(parents=True)

    monkeypatch.setattr(build_deploy_router, "build_and_fix", fake_build_and_fix)
    monkeypatch.setattr(
        build_deploy_router,
        "get_config",
        lambda: {
            "sts2_path": str(game_root),
            "llm": {
                "agent_backend": "codex",
                "model": "gpt-5.4",
            },
        },
    )

    app = FastAPI()
    app.include_router(build_deploy_router.router, prefix="/api")
    project_root = tmp_path / "MyMod"
    output_dir = project_root / "bin" / "Release"
    output_dir.mkdir(parents=True)
    (output_dir / "MyMod.dll").write_bytes(b"dll")
    (output_dir / "MyMod.pck").write_bytes(b"pck")

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/build-deploy") as ws:
            ws.send_text(json.dumps({"project_root": str(project_root)}))
            while True:
                payload = ws.receive_json()
                if payload.get("event") == "done":
                    break

    metadata = json.loads((game_root / "Mods" / "MyMod" / ".server-deploy.json").read_text(encoding="utf-8"))
    assert metadata["schema_version"] == "v1"
    assert metadata["project_name"] == "MyMod"
    assert metadata["job_id"] == 0
    assert metadata["job_item_id"] == 0
    assert metadata["user_id"] == 0
    assert metadata["server_project_ref"] == ""
    assert metadata["source_workspace_root"] == str(project_root)
    assert metadata["deployed_to"] == str(game_root / "Mods" / "MyMod")
    assert metadata["entrypoint"] == "legacy.ws.build_deploy"
    assert metadata["file_names"] == ["MyMod.dll", "MyMod.pck"]
