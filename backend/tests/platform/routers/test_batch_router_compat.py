from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

from fastapi import FastAPI
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

class _DummyPlanItem:
    def __init__(self, **kwargs):
        self.__dict__.update(kwargs)

    def to_dict(self):
        return dict(self.__dict__)


sys.modules.setdefault(
    "agents.planner",
    SimpleNamespace(
        plan_mod=None,
        plan_from_dict=lambda payload: SimpleNamespace(items=[]),
        topological_sort=lambda items: [],
        find_groups=lambda items: [],
        PlanItem=_DummyPlanItem,
    ),
)
sys.modules.setdefault("image.generator", SimpleNamespace(generate_images=None))
sys.modules.setdefault("image.postprocess", SimpleNamespace(process_image=None))
sys.modules.setdefault("image.prompt_adapter", SimpleNamespace(adapt_prompt=None, ImageProvider=str))
sys.modules.setdefault("llm.text_runner", SimpleNamespace(complete_text=None))

import routers.batch_workflow as batch_router


class _FakeContainer:
    def __init__(self, *, runner_enabled: bool, batch_router_service=None) -> None:
        self.platform_migration_flags = SimpleNamespace(
            platform_service_split_enabled=False,
            platform_runner_enabled=runner_enabled,
        )
        self._singletons = {
            "platform.batch_workflow_router_compat_service": batch_router_service,
        }

    def resolve_optional_singleton(self, key: str, default=None):
        return self._singletons.get(key, default)


def test_batch_router_keeps_legacy_plan_execution_when_runner_flag_disabled(monkeypatch):
    monkeypatch.setattr(batch_router, "plan_from_dict", lambda _payload: SimpleNamespace(items=[]))
    monkeypatch.setattr(batch_router, "topological_sort", lambda items: [])
    monkeypatch.setattr(batch_router, "find_groups", lambda items: [])

    app = FastAPI()
    app.state.container = _FakeContainer(runner_enabled=False)
    app.include_router(batch_router.router, prefix="/api")

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/batch") as ws:
            ws.send_text(
                json.dumps(
                    {
                        "action": "start_with_plan",
                        "plan": {"items": []},
                        "project_root": "I:/compat-project",
                    }
                )
            )
            first = ws.receive_json()
            second = ws.receive_json()

    assert first["event"] == "batch_started"
    assert second["event"] == "batch_done"


def test_batch_router_uses_compat_service_when_runner_flag_enabled():
    calls: list[str] = []

    class FakeBatchRouterCompatService:
        async def handle_ws_batch(self, ws) -> None:
            calls.append("compat")
            await ws.accept()
            await ws.receive_text()
            await ws.send_text(json.dumps({"event": "batch_done", "source": "compat"}))

    app = FastAPI()
    app.state.container = _FakeContainer(
        runner_enabled=True,
        batch_router_service=FakeBatchRouterCompatService(),
    )
    app.include_router(batch_router.router, prefix="/api")

    with TestClient(app) as client:
        with client.websocket_connect("/api/ws/batch") as ws:
            ws.send_text(
                json.dumps(
                    {
                        "action": "start_with_plan",
                        "plan": {"items": []},
                        "project_root": "I:/compat-project",
                    }
                )
            )
            payload = ws.receive_json()

    assert payload["source"] == "compat"
    assert calls == ["compat"]
