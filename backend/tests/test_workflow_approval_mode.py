"""Tests for single-asset approval-first workflow helpers."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.approval.application.ports import ActionResult
from approval.runtime import reset_approval_runtime
from approval.service import ApprovalService
from approval.store import InMemoryApprovalStore
from routers import workflow
from app.modules.workflow.application.step import WorkflowStep


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
        return self._incoming.pop(0)


def _assert_approval_precedes_agent_start(messages: list[dict]) -> None:
    approval_index = next(
        idx for idx, message in enumerate(messages) if message["event"] == "approval_pending"
    )
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

    monkeypatch.setattr(workflow, "get_config", lambda: {"llm": {"agent_backend": "codex", "execution_mode": "approval_first"}})

    async def fake_plan_approval_requests(description: str, llm_cfg: dict, project_root: Path):
        assert description == "描述一个遗物"
        assert llm_cfg["execution_mode"] == "approval_first"
        assert project_root == tmp_path
        return "Need approval", []

    async def fake_create_custom_code(**kwargs):
        return "generated"

    monkeypatch.setattr(workflow, "_plan_approval_requests", fake_plan_approval_requests)
    monkeypatch.setattr(workflow, "create_custom_code", fake_create_custom_code)

    await workflow._ws_run_custom_code(
        ws,
        {
            "asset_name": "TestFeature",
            "description": "描述一个遗物",
            "implementation_notes": "",
        },
        tmp_path,
    )

    _assert_approval_precedes_agent_start(ws.messages)


@pytest.mark.asyncio
async def test_provided_image_approval_first_defers_agent_events_until_approve_all(monkeypatch, tmp_path):
    class DummyImage:
        def convert(self, mode: str):
            assert mode == "RGBA"
            return self

    ws = SequencedWs([{"action": "approve_all"}])

    monkeypatch.setattr(workflow, "get_config", lambda: {"llm": {"agent_backend": "codex", "execution_mode": "approval_first"}})

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

    provided_image = tmp_path / "input.png"
    provided_image.write_bytes(b"not-a-real-png")

    from PIL import Image as PILImage

    monkeypatch.setattr(PILImage, "open", lambda path: DummyImage())

    await workflow._ws_run_with_provided_image(
        ws,
        {
            "asset_type": "relic",
            "asset_name": "TestRelic",
            "description": "描述一个遗物",
            "provided_image_path": str(provided_image),
        },
        tmp_path,
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
