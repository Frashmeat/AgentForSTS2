from __future__ import annotations

import asyncio
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.build_deploy_facade_service import BuildDeployFacadeService


class _FakeWebSocket:
    def __init__(self, payload: dict) -> None:
        self._incoming = [json.dumps(payload)]
        self.sent: list[dict] = []
        self.accepted = False

    async def accept(self) -> None:
        self.accepted = True

    async def receive_text(self) -> str:
        return self._incoming.pop(0)

    async def send_text(self, raw: str) -> None:
        self.sent.append(json.loads(raw))


def test_build_deploy_facade_service_returns_error_when_sts2_path_is_configured_but_missing(monkeypatch, tmp_path):
    async def fake_build_and_fix(project_root, stream_callback=None):
        if stream_callback is not None:
            await stream_callback("build ok")
        return True, ""

    monkeypatch.setattr(
        "app.modules.platform.application.services.build_deploy_facade_service.build_and_fix",
        fake_build_and_fix,
    )
    monkeypatch.setattr(
        "app.modules.platform.application.services.build_deploy_facade_service.get_config",
        lambda: {"sts2_path": str(tmp_path / "missing-sts2-root")},
    )

    project_root = tmp_path / "MyMod"
    project_root.mkdir()
    ws = _FakeWebSocket({"project_root": str(project_root)})
    service = BuildDeployFacadeService()

    asyncio.run(service.handle_ws_build_deploy(ws))

    assert ws.sent == [ws.sent[0]]
    assert ws.sent[0]["event"] == "error"


def test_build_deploy_facade_service_syncs_local_props_before_build(monkeypatch, tmp_path):
    called: list[Path] = []

    async def fake_build_and_fix(project_root, stream_callback=None):
        return True, ""

    monkeypatch.setattr(
        "app.modules.platform.application.services.build_deploy_facade_service.build_and_fix",
        fake_build_and_fix,
    )
    monkeypatch.setattr(
        "app.modules.platform.application.services.build_deploy_facade_service.ensure_local_props",
        lambda project_root: called.append(project_root) or True,
    )
    monkeypatch.setattr(
        "app.modules.platform.application.services.build_deploy_facade_service.get_config",
        lambda: {"sts2_path": ""},
    )

    project_root = tmp_path / "MyMod"
    project_root.mkdir()
    ws = _FakeWebSocket({"project_root": str(project_root)})
    service = BuildDeployFacadeService()

    asyncio.run(service.handle_ws_build_deploy(ws))

    assert called == [project_root]
