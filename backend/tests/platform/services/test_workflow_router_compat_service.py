from __future__ import annotations

import asyncio
import json
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace

import pytest
from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

if "litellm" not in sys.modules:
    litellm_stub = ModuleType("litellm")

    async def _unexpected_acompletion(*_args, **_kwargs):
        raise AssertionError("litellm should not be called in workflow compat service tests")

    litellm_stub.acompletion = _unexpected_acompletion
    sys.modules["litellm"] = litellm_stub

if "agents.planner" not in sys.modules:
    planner_stub = ModuleType("agents.planner")

    class _PlanItem:
        def __init__(self, **kwargs):
            self.__dict__.update(kwargs)

    class _ModPlan:
        def __init__(self, mod_name, summary, items):
            self.mod_name = mod_name
            self.summary = summary
            self.items = items

    def _plan_from_dict(payload):
        return _ModPlan(
            mod_name=payload.get("mod_name", ""),
            summary=payload.get("summary", ""),
            items=[_PlanItem(**item) for item in payload.get("items", [])],
        )

    planner_stub.plan_mod = None
    planner_stub.plan_from_dict = _plan_from_dict
    planner_stub.topological_sort = lambda items: []
    planner_stub.find_groups = lambda items: []
    planner_stub.PlanItem = _PlanItem
    sys.modules["agents.planner"] = planner_stub

sys.modules.setdefault("image.generator", SimpleNamespace(generate_images=None))
sys.modules.setdefault("image.postprocess", SimpleNamespace(PROFILES={}, process_image=None))
sys.modules.setdefault("image.prompt_adapter", SimpleNamespace(adapt_prompt=None, ImageProvider=str))
sys.modules.setdefault("llm.text_runner", SimpleNamespace(complete_text=None))

pytest.importorskip("sqlalchemy")

from app.modules.platform.application.services.workflow_router_compat_service import WorkflowRouterCompatService
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import JobEventRecord, JobItemRecord, JobRecord, platform_tables
from app.shared.infra.db.base import Base
import routers.workflow as workflow_router


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


def _session_factory():
    engine = create_engine("sqlite+pysqlite:///:memory:")
    Base.metadata.create_all(engine, tables=platform_tables())
    factory = sessionmaker(bind=engine, autoflush=False, autocommit=False, expire_on_commit=False)
    return engine, factory


def test_workflow_router_compat_service_can_use_platform_main_chain_for_custom_code():
    engine, session_factory = _session_factory()

    async def fake_create_custom_code(**kwargs):
        await kwargs["stream_callback"]("generated chunk")
        return "generated custom code"

    service = WorkflowRouterCompatService(
        session_factory=session_factory,
        create_custom_code_fn=fake_create_custom_code,
    )
    ws = _FakeWebSocket(
        {
            "action": "start",
            "asset_type": "custom_code",
            "asset_name": "CompatCode",
            "description": "通过平台主链生成一段自定义代码",
            "implementation_notes": "写一个最小演示",
            "project_root": "I:/compat-project",
            "user_id": 1001,
        }
    )

    asyncio.run(service.handle_ws_create(ws))

    session = session_factory()
    try:
        job = session.query(JobRecord).one()
        item = session.query(JobItemRecord).one()
        events = session.query(JobEventRecord).order_by(JobEventRecord.id.asc()).all()
    finally:
        session.close()
        engine.dispose()

    assert ws.accepted is True
    assert ws.sent[-1]["event"] == "done"
    assert ws.sent[-1]["success"] is True
    assert job.user_id == 1001
    assert job.status.value == "succeeded"
    assert item.status.value == "succeeded"
    assert [event.event_type for event in events] == [
        "job.created",
        "job.queued",
        "job.item.completed",
        "job.completed",
    ]


def test_workflow_router_compat_service_passes_prefetched_payload_to_legacy_handler(monkeypatch):
    calls: list[tuple[dict, bool]] = []

    async def fake_legacy_handler(ws, *, initial_params=None):
        calls.append((initial_params, ws.accepted))
        await ws.send_text(json.dumps({"event": "done", "source": "legacy"}))

    monkeypatch.setattr(workflow_router, "_handle_legacy_ws_create", fake_legacy_handler)

    service = WorkflowRouterCompatService(session_factory=lambda: None)
    ws = _FakeWebSocket(
        {
            "action": "start",
            "asset_type": "relic",
            "asset_name": "CompatRelic",
            "description": "走 legacy 回退",
            "project_root": "I:/compat-project",
        }
    )

    asyncio.run(service.handle_ws_create(ws))

    assert calls == [
        (
            {
                "action": "start",
                "asset_type": "relic",
                "asset_name": "CompatRelic",
                "description": "走 legacy 回退",
                "project_root": "I:/compat-project",
            },
            True,
        )
    ]
    assert ws.sent[-1]["source"] == "legacy"
