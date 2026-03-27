"""Tests for batch approval-first helpers."""
import asyncio
import importlib
import json
import sys
import types
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

try:
    from fastapi import WebSocketDisconnect
except ModuleNotFoundError:
    import inspect
    import re

    fastapi_stub = types.ModuleType("fastapi")
    fastapi_testclient_stub = types.ModuleType("fastapi.testclient")

    class WebSocketDisconnect(Exception):
        pass

    class HTTPException(Exception):
        def __init__(self, status_code: int, detail: str):
            super().__init__(detail)
            self.status_code = status_code
            self.detail = detail

    class _Route:
        def __init__(self, method: str, path: str, handler):
            self.method = method
            self.path = path
            self.handler = handler
            pattern = re.sub(r"{([^}]+)}", r"(?P<\1>[^/]+)", path)
            self.regex = re.compile(f"^{pattern}$")

    class _DummyRouter:
        def __init__(self, *, prefix: str = ""):
            self.prefix = prefix
            self.routes: list[_Route] = []

        def _register(self, method: str, path: str, handler):
            self.routes.append(_Route(method, f"{self.prefix}{path}", handler))
            return handler

        def get(self, path: str, *_args, **_kwargs):
            return lambda fn: self._register("GET", path, fn)

        def post(self, path: str, *_args, **_kwargs):
            return lambda fn: self._register("POST", path, fn)

        def websocket(self, *_args, **_kwargs):
            return lambda fn: fn

    class FastAPI:
        def __init__(self):
            self.routes: list[_Route] = []

        def include_router(self, router: _DummyRouter, prefix: str = ""):
            for route in router.routes:
                self.routes.append(_Route(route.method, f"{prefix}{route.path}", route.handler))

    class _Response:
        def __init__(self, status_code: int, payload):
            self.status_code = status_code
            self._payload = payload

        def json(self):
            return self._payload

    class TestClient:
        def __init__(self, app: FastAPI):
            self.app = app

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, tb):
            return False

        def get(self, path: str):
            return self._request("GET", path, None)

        def post(self, path: str, json: dict | None = None):
            return self._request("POST", path, json)

        def _request(self, method: str, path: str, body: dict | None):
            for route in self.app.routes:
                if route.method != method:
                    continue
                match = route.regex.match(path)
                if match is None:
                    continue

                kwargs = dict(match.groupdict())
                signature = inspect.signature(route.handler)
                if "body" in signature.parameters:
                    kwargs["body"] = body or {}

                try:
                    payload = route.handler(**kwargs)
                    return _Response(200, payload)
                except HTTPException as exc:
                    return _Response(exc.status_code, {"detail": exc.detail})

            return _Response(404, {"detail": "Not Found"})

    fastapi_stub.APIRouter = _DummyRouter
    fastapi_stub.FastAPI = FastAPI
    fastapi_stub.HTTPException = HTTPException
    fastapi_stub.WebSocket = object
    fastapi_stub.WebSocketDisconnect = WebSocketDisconnect
    fastapi_testclient_stub.TestClient = TestClient
    sys.modules["fastapi"] = fastapi_stub
    sys.modules["fastapi.testclient"] = fastapi_testclient_stub

try:
    importlib.import_module("litellm")
except ModuleNotFoundError:
    litellm_stub = types.ModuleType("litellm")

    async def _unexpected_acompletion(*_args, **_kwargs):
        raise AssertionError("litellm.acompletion should not be called in batch approval tests")

    litellm_stub.acompletion = _unexpected_acompletion
    sys.modules["litellm"] = litellm_stub

try:
    importlib.import_module("agents.code_agent")
except ModuleNotFoundError:
    code_agent_stub = types.ModuleType("agents.code_agent")

    async def _unexpected_code_agent(*_args, **_kwargs):
        raise AssertionError("code agent stub should be monkeypatched in batch approval tests")

    def _unexpected_code_agent_sync(*_args, **_kwargs):
        raise AssertionError("code agent stub should be monkeypatched in batch approval tests")

    code_agent_stub.build_and_fix = _unexpected_code_agent
    code_agent_stub.create_asset = _unexpected_code_agent
    code_agent_stub.create_asset_group = _unexpected_code_agent
    code_agent_stub.create_custom_code = _unexpected_code_agent
    code_agent_stub.create_mod_project = _unexpected_code_agent
    code_agent_stub.get_decompiled_src_path = _unexpected_code_agent_sync
    code_agent_stub.package_mod = _unexpected_code_agent
    code_agent_stub.run_claude_code = _unexpected_code_agent
    sys.modules["agents.code_agent"] = code_agent_stub

try:
    importlib.import_module("image.generator")
except ModuleNotFoundError:
    image_generator_stub = types.ModuleType("image.generator")

    async def _unexpected_generate_images(*_args, **_kwargs):
        raise AssertionError("generate_images should not be called in batch approval tests")

    image_generator_stub.generate_images = _unexpected_generate_images
    sys.modules["image.generator"] = image_generator_stub

try:
    importlib.import_module("image.postprocess")
except ModuleNotFoundError:
    image_postprocess_stub = types.ModuleType("image.postprocess")
    image_postprocess_stub.PROFILES = {}
    image_postprocess_stub.process_image = lambda *_args, **_kwargs: []
    sys.modules["image.postprocess"] = image_postprocess_stub

try:
    importlib.import_module("image.prompt_adapter")
except ModuleNotFoundError:
    prompt_adapter_stub = types.ModuleType("image.prompt_adapter")

    async def _unexpected_adapt_prompt(*_args, **_kwargs):
        raise AssertionError("adapt_prompt should not be called in batch approval tests")

    prompt_adapter_stub.adapt_prompt = _unexpected_adapt_prompt
    prompt_adapter_stub.ImageProvider = str
    sys.modules["image.prompt_adapter"] = prompt_adapter_stub

from approval.runtime import reset_approval_runtime
from approval.models import ActionRequest
from agents.planner import ModPlan, PlanItem
from app.modules.workflow.application.step import WorkflowStep
from routers import batch_workflow


@pytest.fixture(autouse=True)
def approval_runtime_isolation():
    reset_approval_runtime()
    try:
        yield
    finally:
        reset_approval_runtime()


def test_plan_group_approval_requests_creates_pending_actions(monkeypatch, tmp_path):
    async def run():
        group = [
            PlanItem(
                id="power_burn",
                type="power",
                name="BurnPower",
                description="一个灼烧 power",
                implementation_notes="实现 PowerModel",
                needs_image=False,
            ),
            PlanItem(
                id="card_ignite",
                type="card",
                name="IgniteCard",
                description="引用 BurnPower 的卡牌",
                implementation_notes="调用 BurnPower",
                needs_image=False,
                depends_on=["power_burn"],
            ),
        ]

        async def fake_complete_text(prompt: str, llm_cfg: dict, cwd: Path | None = None) -> str:
            assert "Output ONLY JSON" in prompt
            assert cwd == tmp_path
            return json.dumps(
                {
                    "summary": "Need approval for this asset group",
                    "actions": [
                        {
                            "kind": "write_file",
                            "title": "Write grouped source",
                            "reason": "Need generated files for the group",
                            "payload": {"path": "Cards/IgniteCard.cs"},
                        }
                    ],
                }
            )

        monkeypatch.setattr(batch_workflow, "complete_text", fake_complete_text)

        summary, actions = await batch_workflow._plan_group_approval_requests(
            group,
            {"agent_backend": "codex", "execution_mode": "approval_first"},
            tmp_path,
        )

        assert summary == "Need approval for this asset group"
        assert len(actions) == 1
        assert actions[0].source_workflow == "batch"
        assert actions[0].status == "pending"

    asyncio.run(run())


def test_send_item_approval_pending_emits_expected_event():
    async def run():
        class DummyWs:
            def __init__(self):
                self.messages: list[dict] = []

            async def send_text(self, text: str):
                self.messages.append(json.loads(text))

        ws = DummyWs()
        await batch_workflow._send_item_approval_pending(ws, "card_ignite", "Need approval", [])

        assert ws.messages[-1]["event"] == "item_approval_pending"
        assert ws.messages[-1]["item_id"] == "card_ignite"
        assert ws.messages[-1]["summary"] == "Need approval"
        assert ws.messages[-1]["requests"] == []

    asyncio.run(run())


def test_approval_first_pending_group_does_not_emit_batch_done_early(monkeypatch, tmp_path):
    async def run():
        class DummyWs:
            def __init__(self, queued_messages: list[str]):
                self._queued_messages = list(queued_messages)
                self._receive_calls = 0
                self.messages: list[dict] = []

            async def accept(self):
                return None

            async def send_text(self, text: str):
                self.messages.append(json.loads(text))

            async def receive_text(self) -> str:
                if self._queued_messages:
                    return self._queued_messages.pop(0)

                self._receive_calls += 1
                if self._receive_calls == 1:
                    await asyncio.sleep(1.1)
                    return json.dumps({})

                raise WebSocketDisconnect()

        monkeypatch.setattr(
            batch_workflow,
            "get_config",
            lambda: {
                "llm": {"agent_backend": "codex", "execution_mode": "approval_first"},
                "image_gen": {"provider": "bfl", "concurrency": 1},
                "migration": {},
            },
        )

        async def fake_plan_group_approval_requests(group, llm_cfg, project_root):
            assert [item.id for item in group] == ["power_burn", "card_ignite"]
            assert llm_cfg["execution_mode"] == "approval_first"
            assert project_root == tmp_path
            return "Need approval", []

        monkeypatch.setattr(
            batch_workflow,
            "_plan_group_approval_requests",
            fake_plan_group_approval_requests,
        )

        (tmp_path / "TestMod.csproj").write_text("<Project />", encoding="utf-8")

        plan = ModPlan(
            mod_name="TestMod",
            summary="approval-first batch",
            items=[
                PlanItem(
                    id="power_burn",
                    type="power",
                    name="BurnPower",
                    description="一个灼烧 power",
                    implementation_notes="实现 PowerModel",
                    needs_image=False,
                ),
                PlanItem(
                    id="card_ignite",
                    type="card",
                    name="IgniteCard",
                    description="引用 BurnPower 的卡牌",
                    implementation_notes="调用 BurnPower",
                    needs_image=False,
                    depends_on=["power_burn"],
                ),
            ],
        )
        ws = DummyWs(
            [
                json.dumps(
                    {
                        "action": "start_with_plan",
                        "project_root": str(tmp_path),
                        "plan": plan.to_dict(),
                    }
                )
            ]
        )

        await batch_workflow.ws_batch(ws)

        events = [message["event"] for message in ws.messages]
        assert "item_approval_pending" in events
        assert "item_done" not in events
        assert "batch_done" not in events

    asyncio.run(run())


def test_approval_first_group_can_resume_after_approve_all(monkeypatch, tmp_path):
    async def run():
        request = ActionRequest(
            kind="write_file",
            title="Write grouped source",
            reason="Need generated files for the group",
            payload={"path": "Scripts/Helper.cs", "content": "// generated"},
            risk_level="medium",
            requires_approval=True,
            source_backend="codex",
            source_workflow="batch",
        )

        class FakeApprovalStore:
            def __init__(self):
                self.approved_ids: list[str] = []

            def approve_request(self, action_id: str):
                self.approved_ids.append(action_id)
                request.status = "approved"
                return request

        class FakeApprovalService:
            def __init__(self):
                self.store = FakeApprovalStore()
                self.executed_ids: list[str] = []

            async def execute_request(self, action_id: str):
                self.executed_ids.append(action_id)
                request.status = "succeeded"
                return request

        fake_service = FakeApprovalService()
        create_calls: list[tuple[str, str]] = []

        class DummyWs:
            def __init__(self, queued_messages: list[str]):
                self._queued_messages = list(queued_messages)
                self.messages: list[dict] = []

            async def accept(self):
                return None

            async def send_text(self, text: str):
                self.messages.append(json.loads(text))

            async def receive_text(self) -> str:
                if self._queued_messages:
                    return self._queued_messages.pop(0)
                while not any(message.get("event") == "batch_done" for message in self.messages):
                    await asyncio.sleep(0.05)
                raise WebSocketDisconnect()

        monkeypatch.setattr(
            batch_workflow,
            "get_config",
            lambda: {
                "llm": {"agent_backend": "codex", "execution_mode": "approval_first"},
                "image_gen": {"provider": "bfl", "concurrency": 1},
                "migration": {},
            },
        )
        monkeypatch.setattr(batch_workflow, "get_approval_service", lambda: fake_service)

        async def fake_plan_group_approval_requests(group, llm_cfg, project_root):
            assert [item.id for item in group] == ["helper_logic"]
            assert llm_cfg["execution_mode"] == "approval_first"
            assert project_root == tmp_path
            return "Need approval", [request]

        async def fake_create_custom_code(description, implementation_notes, name, project_root, stream_callback, skip_build=True):
            create_calls.append((name, implementation_notes))
            return "generated custom code"

        monkeypatch.setattr(
            batch_workflow,
            "_plan_group_approval_requests",
            fake_plan_group_approval_requests,
        )
        monkeypatch.setattr(batch_workflow, "create_custom_code", fake_create_custom_code)

        (tmp_path / "TestMod.csproj").write_text("<Project />", encoding="utf-8")

        plan = ModPlan(
            mod_name="TestMod",
            summary="approval-first batch",
            items=[
                PlanItem(
                    id="helper_logic",
                    type="custom_code",
                    name="HelperLogic",
                    description="一个帮助器",
                    implementation_notes="实现一个 helper",
                    needs_image=False,
                ),
            ],
        )
        ws = DummyWs(
            [
                json.dumps(
                    {
                        "action": "start_with_plan",
                        "project_root": str(tmp_path),
                        "plan": plan.to_dict(),
                    }
                ),
                json.dumps({"action": "approve_all", "item_id": "helper_logic"}),
                json.dumps({"action": "resume", "item_id": "helper_logic"}),
            ]
        )

        await batch_workflow.ws_batch(ws)

        events = [message["event"] for message in ws.messages]
        assert "item_approval_pending" in events
        assert "item_done" in events
        assert "batch_done" in events
        assert fake_service.store.approved_ids == [request.action_id]
        assert fake_service.executed_ids == [request.action_id]
        assert create_calls == [("HelperLogic", "实现一个 helper")]

    asyncio.run(run())


def test_batch_workflow_uses_dependency_graph_and_limited_parallelism():
    async def run():
        class DummyWs:
            def __init__(self):
                self.messages: list[dict] = []

            async def send_text(self, text: str):
                self.messages.append(json.loads(text))

        async def image_step(context):
            return {"data": {"item_id": "card_ignite", "image": "abc"}}

        async def done_step(context):
            return {"data": {"item_id": "card_ignite", "success": True}}

        ws = DummyWs()
        await batch_workflow._run_batch_asset_engine(
            ws,
            [
                WorkflowStep(name="done", handler=done_step, depends_on=["image_ready"]),
                WorkflowStep(name="image_ready", handler=image_step),
            ],
            max_concurrency=2,
        )

        assert ws.messages[0]["event"] == "item_image_ready"
        assert ws.messages[-1]["event"] == "item_done"

    asyncio.run(run())
