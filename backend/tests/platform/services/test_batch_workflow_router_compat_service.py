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
        raise AssertionError("litellm should not be called in batch compat service tests")

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
            mod_name=payload["mod_name"],
            summary=payload["summary"],
            items=[_PlanItem(**item) for item in payload["items"]],
        )

    planner_stub.plan_mod = None
    planner_stub.plan_from_dict = _plan_from_dict
    planner_stub.topological_sort = lambda items: []
    planner_stub.find_groups = lambda items: []
    planner_stub.PlanItem = _PlanItem
    sys.modules["agents.planner"] = planner_stub

sys.modules.setdefault("image.generator", SimpleNamespace(generate_images=None))
sys.modules.setdefault("image.postprocess", SimpleNamespace(process_image=None))
sys.modules.setdefault("image.prompt_adapter", SimpleNamespace(adapt_prompt=None, ImageProvider=str))
sys.modules.setdefault("llm.text_runner", SimpleNamespace(complete_text=None))

pytest.importorskip("sqlalchemy")

from app.modules.platform.application.services.batch_workflow_router_compat_service import BatchWorkflowRouterCompatService
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import JobEventRecord, JobItemRecord, JobRecord, platform_tables
from app.shared.infra.db.base import Base
import routers.batch_workflow as batch_router


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


def test_batch_workflow_router_compat_service_can_use_platform_main_chain_for_custom_code_plan():
    engine, session_factory = _session_factory()

    async def fake_create_custom_code(**kwargs):
        await kwargs["stream_callback"](f"generated {kwargs['name']}")
        return f"generated {kwargs['name']}"

    service = BatchWorkflowRouterCompatService(
        session_factory=session_factory,
        create_custom_code_fn=fake_create_custom_code,
    )
    ws = _FakeWebSocket(
        {
            "action": "start_with_plan",
            "project_root": "I:/compat-project",
            "user_id": 1001,
            "plan": {
                "mod_name": "CompatMod",
                "summary": "最小批量主链闭环",
                "items": [
                    {
                        "id": "custom_a",
                        "type": "custom_code",
                        "name": "CustomA",
                        "description": "第一个自定义代码项",
                        "implementation_notes": "实现 A",
                        "needs_image": False,
                        "depends_on": [],
                    },
                    {
                        "id": "custom_b",
                        "type": "custom_code",
                        "name": "CustomB",
                        "description": "第二个自定义代码项",
                        "implementation_notes": "实现 B",
                        "needs_image": False,
                        "depends_on": ["custom_a"],
                    },
                ],
            },
        }
    )

    asyncio.run(service.handle_ws_batch(ws))

    session = session_factory()
    try:
        job = session.query(JobRecord).one()
        items = session.query(JobItemRecord).order_by(JobItemRecord.item_index.asc()).all()
        events = session.query(JobEventRecord).order_by(JobEventRecord.id.asc()).all()
    finally:
        session.close()
        engine.dispose()

    assert ws.accepted is True
    assert ws.sent[0]["event"] == "batch_started"
    assert ws.sent[-1]["event"] == "batch_done"
    assert ws.sent[-1]["success_count"] == 2
    assert job.user_id == 1001
    assert job.status.value == "succeeded"
    assert [item.status.value for item in items] == ["succeeded", "succeeded"]
    assert [event.event_type for event in events] == [
        "job.created",
        "job.queued",
        "job.item.completed",
        "job.item.completed",
        "job.completed",
    ]


def test_batch_workflow_router_compat_service_passes_prefetched_payload_to_legacy_handler(monkeypatch):
    calls: list[tuple[dict, bool]] = []

    async def fake_legacy_handler(ws, *, initial_params=None):
        calls.append((initial_params, ws.accepted))
        await ws.send_text(json.dumps({"event": "batch_done", "source": "legacy"}))

    monkeypatch.setattr(batch_router, "_handle_legacy_ws_batch", fake_legacy_handler)

    service = BatchWorkflowRouterCompatService(session_factory=lambda: None)
    ws = _FakeWebSocket(
        {
            "action": "start",
            "requirements": "做一个测试 Mod",
            "project_root": "I:/compat-project",
        }
    )

    asyncio.run(service.handle_ws_batch(ws))

    assert calls == [
        (
            {
                "action": "start",
                "requirements": "做一个测试 Mod",
                "project_root": "I:/compat-project",
            },
            True,
        )
    ]
    assert ws.sent[-1]["source"] == "legacy"


def test_batch_workflow_router_compat_service_emits_structured_error_when_platform_chain_fails():
    engine, session_factory = _session_factory()

    async def fake_create_custom_code(**_kwargs):
        raise RuntimeError("platform batch custom code failed")

    service = BatchWorkflowRouterCompatService(
        session_factory=session_factory,
        create_custom_code_fn=fake_create_custom_code,
    )
    ws = _FakeWebSocket(
        {
            "action": "start_with_plan",
            "project_root": "I:/compat-project",
            "user_id": 1001,
            "plan": {
                "mod_name": "CompatMod",
                "summary": "最小批量主链闭环",
                "items": [
                    {
                        "id": "custom_a",
                        "type": "custom_code",
                        "name": "CustomA",
                        "description": "第一个自定义代码项",
                        "implementation_notes": "实现 A",
                        "needs_image": False,
                        "depends_on": [],
                    }
                ],
            },
        }
    )

    try:
        asyncio.run(service.handle_ws_batch(ws))
    finally:
        engine.dispose()

    assert ws.accepted is True
    assert ws.sent[-1]["event"] == "error"
    assert ws.sent[-1]["code"] == "batch_workflow_failed"
    assert ws.sent[-1]["message"] == "platform batch custom code failed"
    assert ws.sent[-1]["detail"] == "platform batch custom code failed"
    assert "traceback" in ws.sent[-1]
