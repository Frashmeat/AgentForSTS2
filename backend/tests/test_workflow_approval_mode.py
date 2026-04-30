"""Tests for single-asset approval-first workflow helpers."""

import asyncio
import json
import sys
import types
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))


# fastapi 不再 stub —— 真 fastapi 在测试容器里可用，避免污染 sys.modules['fastapi'] 影响其他测试模块。
# litellm 等保留 stub 是为了避免真发外部请求；这些 setdefault 只在真模块未安装时才占位。
sys.modules.setdefault("litellm", types.SimpleNamespace(acompletion=None))
sys.modules.setdefault("image.generator", types.SimpleNamespace(generate_images=lambda *args, **kwargs: None))
sys.modules.setdefault(
    "image.postprocess", types.SimpleNamespace(PROFILES={}, process_image=lambda *_args, **_kwargs: [])
)
sys.modules.setdefault(
    "image.prompt_adapter", types.SimpleNamespace(adapt_prompt=lambda *args, **kwargs: None, ImageProvider=str)
)

from app.modules.approval.application.ports import ActionResult
from app.modules.approval.application.services import ApprovalService
from app.modules.approval.infra.in_memory_store import InMemoryApprovalStore
from app.modules.approval.runtime import reset_approval_runtime
from app.modules.workflow.application.step import WorkflowStep
from routers import workflow


@pytest.fixture(autouse=True)
def approval_runtime_isolation():
    reset_approval_runtime()
    try:
        yield
    finally:
        reset_approval_runtime()


@pytest.mark.asyncio
async def test_plan_approval_requests_creates_pending_actions(monkeypatch, tmp_path):
    async def fake_complete_text(prompt: str, llm_cfg: dict, cwd: Path | None = None) -> str:
        assert "Output ONLY JSON" in prompt
        assert cwd == tmp_path
        return json.dumps(
            {
                "summary": "Need approval before modifying project files",
                "actions": [
                    {
                        "kind": "write_file",
                        "title": "Write card source",
                        "reason": "Need generated implementation",
                        "payload": {"path": "Cards/TestCard.cs"},
                    }
                ],
            }
        )

    monkeypatch.setattr(workflow, "complete_text", fake_complete_text)

    llm_cfg = {"agent_backend": "codex", "execution_mode": "approval_first"}
    summary, actions = await workflow._plan_approval_requests(
        "描述一个遗物",
        llm_cfg,
        tmp_path,
    )

    assert summary == "Need approval before modifying project files"
    assert len(actions) == 1
    assert actions[0].kind == "write_file"
    assert actions[0].status == "pending"
    assert actions[0].source_workflow == "single_asset"


@pytest.mark.asyncio
async def test_send_approval_pending_emits_expected_event():
    class DummyWs:
        def __init__(self):
            self.messages: list[dict] = []

        async def send_text(self, text: str):
            self.messages.append(json.loads(text))

    ws = DummyWs()
    await workflow._send_approval_pending(ws, "Need approval", [])

    assert ws.messages[-1]["event"] == "approval_pending"
    assert ws.messages[-1]["summary"] == "Need approval"
    assert ws.messages[-1]["requests"] == []


def test_classify_workflow_error_maps_auth_failures_to_specific_codes():
    code, message = workflow._classify_workflow_error(
        RuntimeError("Claude CLI 退出码 1\nFailed to authenticate. API Error: 401 invalid token")
    )
    assert code == "workflow_api_key_invalid"
    assert "API Key 无效" in message

    code, message = workflow._classify_workflow_error(
        RuntimeError("Claude CLI 退出码 1\nFailed to authenticate. API Error: 403 forbidden")
    )
    assert code == "workflow_api_key_forbidden"
    assert "API Key 无权限" in message


@pytest.mark.asyncio
async def test_maybe_await_approval_continues_main_flow_after_approve_all(monkeypatch, tmp_path):
    class DummyWs:
        def __init__(self):
            self.messages: list[dict] = []
            self._incoming = [json.dumps({"action": "approve_all"})]

        async def send_text(self, text: str):
            self.messages.append(json.loads(text))

        async def receive_text(self) -> str:
            return self._incoming.pop(0)

    store = InMemoryApprovalStore()
    service = ApprovalService(store, executor=None)
    actions = service.create_requests_from_plan(
        {
            "summary": "Need approval before modifying project files",
            "actions": [
                {
                    "kind": "write_file",
                    "title": "Write card source",
                    "reason": "Need generated implementation",
                    "payload": {"path": "Cards/TestCard.cs"},
                }
            ],
        },
        source_backend="codex",
        source_workflow="single_asset",
    )

    async def fake_plan_approval_requests(description: str, llm_cfg: dict, project_root: Path):
        assert description == "描述一个遗物"
        assert llm_cfg["execution_mode"] == "approval_first"
        assert project_root == tmp_path
        return "Need approval before modifying project files", actions

    monkeypatch.setattr(workflow, "_plan_approval_requests", fake_plan_approval_requests)
    monkeypatch.setattr(workflow, "get_approval_service", lambda: service)

    ws = DummyWs()
    proceed, approval_output = await workflow._maybe_await_approval(
        ws,
        "描述一个遗物",
        {"agent_backend": "codex", "execution_mode": "approval_first"},
        tmp_path,
    )

    assert proceed is True
    assert approval_output is None
    assert store.get_request(actions[0].action_id).status == "pending"
    assert any(message["event"] == "approval_pending" for message in ws.messages)
    assert any(message["event"] == "stage_update" and message["stage"] == "agent_running" for message in ws.messages)


class SequencedWs:
    def __init__(self, incoming: list[dict] | None = None):
        self.messages: list[dict] = []
        self._incoming = [json.dumps(item) for item in (incoming or [])]
        self.client = ("testclient", 0)

    async def accept(self):
        return None

    async def send_text(self, text: str):
        self.messages.append(json.loads(text))

    async def receive_text(self) -> str:
        if not self._incoming:
            # 队列耗尽时永远挂起；让主流程结束时主动 cancel _read_ws_controls 后台 task
            await asyncio.Event().wait()
        return self._incoming.pop(0)


def _make_helper_args(ws):
    """生成 _ws_run_custom_code / _ws_run_with_provided_image 需要的 cancellation / receive_control / run_cancellable。"""
    cancellation = workflow._WsCancellation()

    async def receive_control() -> dict:
        return json.loads(await ws.receive_text())

    async def run_cancellable(awaitable):
        return await awaitable

    return cancellation, receive_control, run_cancellable


def _assert_approval_precedes_agent_start(messages: list[dict]) -> None:
    approval_index = next(idx for idx, message in enumerate(messages) if message["event"] == "approval_pending")
    agent_stage_index = next(
        idx
        for idx, message in enumerate(messages)
        if message["event"] == "stage_update" and message.get("stage") == "agent_running"
    )
    agent_progress_index = next(
        idx
        for idx, message in enumerate(messages)
        if message["event"] == "progress" and message.get("message") == "审批通过，Code Agent 开始生成代码..."
    )

    assert approval_index < agent_stage_index
    assert approval_index < agent_progress_index
    assert messages[agent_stage_index]["message"] == "审批通过，开始生成代码..."


@pytest.mark.asyncio
async def test_custom_code_approval_first_defers_agent_events_until_approve_all(monkeypatch, tmp_path):
    ws = SequencedWs([{"action": "approve_all"}])

    monkeypatch.setattr(
        workflow, "get_config", lambda: {"llm": {"agent_backend": "codex", "execution_mode": "approval_first"}}
    )

    async def fake_plan_approval_requests(description: str, llm_cfg: dict, project_root: Path):
        assert description == "描述一个遗物"
        assert llm_cfg["execution_mode"] == "approval_first"
        assert project_root == tmp_path
        return "Need approval", []

    async def fake_create_custom_code(**kwargs):
        return "generated"

    monkeypatch.setattr(workflow, "_plan_approval_requests", fake_plan_approval_requests)
    monkeypatch.setattr(workflow, "create_custom_code", fake_create_custom_code)

    cancellation, receive_control, run_cancellable = _make_helper_args(ws)
    await workflow._ws_run_custom_code(
        ws,
        {
            "asset_name": "TestFeature",
            "description": "描述一个遗物",
            "implementation_notes": "",
        },
        tmp_path,
        cancellation,
        receive_control,
        run_cancellable,
    )

    _assert_approval_precedes_agent_start(ws.messages)


@pytest.mark.asyncio
async def test_provided_image_approval_first_defers_agent_events_until_approve_all(monkeypatch, tmp_path):
    class DummyImage:
        def convert(self, mode: str):
            assert mode == "RGBA"
            return self

    ws = SequencedWs([{"action": "approve_all"}])

    monkeypatch.setattr(
        workflow, "get_config", lambda: {"llm": {"agent_backend": "codex", "execution_mode": "approval_first"}}
    )

    async def fake_plan_approval_requests(description: str, llm_cfg: dict, project_root: Path):
        assert description == "描述一个遗物"
        assert llm_cfg["execution_mode"] == "approval_first"
        assert project_root == tmp_path
        return "Need approval", []

    async def fake_run_postprocess(img, asset_type, asset_name, project_root: Path):
        assert img is not None
        assert asset_type == "relic"
        assert asset_name == "TestRelic"
        assert project_root == tmp_path
        return [tmp_path / "TestRelic.png"]

    async def fake_create_asset(*args, **kwargs):
        return "generated"

    monkeypatch.setattr(workflow, "_plan_approval_requests", fake_plan_approval_requests)
    monkeypatch.setattr(workflow, "_run_postprocess", fake_run_postprocess)
    monkeypatch.setattr(workflow, "create_asset", fake_create_asset)

    pil_module = types.ModuleType("PIL")
    image_module = types.ModuleType("PIL.Image")
    image_module.open = lambda stream: DummyImage()
    pil_module.Image = image_module
    monkeypatch.setitem(sys.modules, "PIL", pil_module)
    monkeypatch.setitem(sys.modules, "PIL.Image", image_module)

    cancellation, receive_control, run_cancellable = _make_helper_args(ws)
    await workflow._ws_run_with_provided_image(
        ws,
        {
            "asset_type": "relic",
            "asset_name": "TestRelic",
            "description": "描述一个遗物",
            "provided_image_b64": "ZmFrZS1pbWFnZS1ieXRlcw==",
            "provided_image_name": "input.png",
        },
        tmp_path,
        cancellation,
        receive_control,
        run_cancellable,
    )

    _assert_approval_precedes_agent_start(ws.messages)


@pytest.mark.asyncio
async def test_ws_create_approval_first_defers_agent_events_until_approve_all(monkeypatch, tmp_path):
    class DummyImage:
        def save(self, buf, format: str):
            assert format == "PNG"
            buf.write(b"png-bytes")

    ws = SequencedWs(
        [
            {
                "action": "start",
                "asset_type": "relic",
                "asset_name": "TestRelic",
                "description": "描述一个遗物",
                "project_root": str(tmp_path),
            },
            {"action": "confirm"},
            {"action": "select", "index": 0},
            {"action": "approve_all"},
        ]
    )

    monkeypatch.setattr(
        workflow,
        "get_config",
        lambda: {
            "llm": {"agent_backend": "codex", "execution_mode": "approval_first"},
            "image_gen": {"provider": "bfl", "batch_size": 1},
        },
    )

    async def fake_adapt_prompt(*args, **kwargs):
        return {"prompt": "preview prompt", "negative_prompt": ""}

    async def fake_generate_images(*args, **kwargs):
        return [DummyImage()]

    async def fake_run_postprocess(img, asset_type, asset_name, project_root: Path):
        assert asset_type == "relic"
        assert asset_name == "TestRelic"
        assert project_root == tmp_path
        return [tmp_path / "TestRelic.png"]

    async def fake_plan_approval_requests(description: str, llm_cfg: dict, project_root: Path):
        assert description == "描述一个遗物"
        assert llm_cfg["execution_mode"] == "approval_first"
        assert project_root == tmp_path
        return "Need approval", []

    async def fake_create_asset(*args, **kwargs):
        return "generated"

    monkeypatch.setattr(workflow, "adapt_prompt", fake_adapt_prompt)
    monkeypatch.setattr(workflow, "generate_images", fake_generate_images)
    monkeypatch.setattr(workflow, "_run_postprocess", fake_run_postprocess)
    monkeypatch.setattr(workflow, "_plan_approval_requests", fake_plan_approval_requests)
    monkeypatch.setattr(workflow, "create_asset", fake_create_asset)

    await workflow.ws_create(ws)

    _assert_approval_precedes_agent_start(ws.messages)


@pytest.mark.asyncio
async def test_single_asset_workflow_uses_engine_and_emits_standard_events():
    class DummyWs:
        def __init__(self):
            self.messages: list[dict] = []

        async def send_text(self, text: str):
            self.messages.append(json.loads(text))

    async def emit_preview(context):
        return {"data": {"prompt": "preview prompt"}}

    async def emit_done(context):
        return {"data": {"success": True}}

    ws = DummyWs()
    await workflow._run_single_asset_engine(
        ws,
        [
            WorkflowStep(name="prompt_preview", handler=emit_preview),
            WorkflowStep(name="done", handler=emit_done),
        ],
    )

    assert ws.messages[0]["event"] == "prompt_preview"
    assert ws.messages[-1]["event"] == "done"


def test_publish_standard_event_preserves_structured_error_fields():
    async def run():
        class DummyWs:
            def __init__(self):
                self.messages: list[dict] = []

            async def send_text(self, text: str):
                self.messages.append(json.loads(text))

        ws = DummyWs()

        await workflow._publish_standard_event(
            ws,
            types.SimpleNamespace(
                stage="error",
                payload={
                    "code": "workflow_step_failed",
                    "message": "构建失败",
                    "detail": "dotnet publish exited with code 1",
                    "traceback": "stacktrace",
                },
            ),
        )

        assert ws.messages == [
            {
                "event": "error",
                "code": "workflow_step_failed",
                "message": "构建失败",
                "detail": "dotnet publish exited with code 1",
                "traceback": "stacktrace",
            }
        ]

    import asyncio

    asyncio.run(run())
