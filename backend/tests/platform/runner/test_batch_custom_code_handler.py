from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.batch_custom_code_handler import execute_batch_custom_code_step


def test_execute_batch_custom_code_step_builds_prompt_and_delegates_to_text_generation(monkeypatch):
    captured: dict[str, object] = {}

    class _FakePromptLoader:
        def render(self, template_name: str, variables: dict[str, object]) -> str:
            captured["template_name"] = template_name
            captured["variables"] = dict(variables)
            return f"prompt:{variables['item_name']}|{variables['description']}"

    async def fake_text_step(request: StepExecutionRequest):
        captured["request"] = request
        return {
            "text": "摘要：建议先实现 BattleScriptManager\n1. 增加战斗入口监听。",
            "provider": request.execution_binding.provider,
            "model": request.execution_binding.model,
        }

    monkeypatch.setattr(
        "app.modules.platform.runner.batch_custom_code_handler._PROMPT_LOADER",
        _FakePromptLoader(),
    )

    result = asyncio.run(
        execute_batch_custom_code_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="batch.custom_code.plan",
                step_id="batch-custom-code-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "BattleScriptManager",
                    "description": "实现一个战斗阶段脚本管理器",
                    "implementation_notes": "维护状态机并派发事件",
                    "affected_targets": ["Scripts/BattleScriptManager.cs"],
                    "depends_on": ["battle_bootstrap"],
                },
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                ),
            ),
            text_step_executor=fake_text_step,
        )
    )

    forwarded = captured["request"]
    assert isinstance(forwarded, StepExecutionRequest)
    assert captured["template_name"] == "runtime_agent.platform_batch_custom_code_server_user"
    assert captured["variables"]["item_name"] == "BattleScriptManager"
    assert "- Scripts/BattleScriptManager.cs" in str(captured["variables"]["affected_targets"])
    assert forwarded.step_type == "text.generate"
    assert forwarded.input_payload["prompt"] == "prompt:BattleScriptManager|实现一个战斗阶段脚本管理器"
    assert result["text"] == "建议先实现 BattleScriptManager"
    assert result["analysis"].startswith("摘要：建议先实现 BattleScriptManager")
    assert result["item_name"] == "BattleScriptManager"


def test_execute_batch_custom_code_step_requires_descriptive_input():
    try:
        asyncio.run(
            execute_batch_custom_code_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="batch.custom_code.plan",
                    step_id="batch-custom-code-2",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"item_name": "EmptyItem"},
                    execution_binding=StepExecutionBinding(
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                )
            )
        )
    except ValueError as error:
        assert str(error) == "custom_code server task requires descriptive input"
    else:
        raise AssertionError("expected ValueError when descriptive input is missing")


def test_execute_batch_custom_code_step_requires_item_name():
    try:
        asyncio.run(
            execute_batch_custom_code_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="batch.custom_code.plan",
                    step_id="batch-custom-code-3",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"description": "实现一个战斗阶段脚本管理器"},
                    execution_binding=StepExecutionBinding(
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                )
            )
        )
    except ValueError as error:
        assert str(error) == "custom_code server task requires item_name"
    else:
        raise AssertionError("expected ValueError when item_name is missing")


def test_execute_batch_custom_code_step_supports_single_generate_payload_shape(monkeypatch):
    captured: dict[str, object] = {}

    class _FakePromptLoader:
        def render(self, template_name: str, variables: dict[str, object]) -> str:
            captured["variables"] = dict(variables)
            return f"prompt:{variables['item_name']}"

    async def fake_text_step(request: StepExecutionRequest):
        captured["request"] = request
        return {
            "text": "摘要：先补单资产 custom_code 的核心类骨架",
            "provider": request.execution_binding.provider,
            "model": request.execution_binding.model,
        }

    monkeypatch.setattr(
        "app.modules.platform.runner.batch_custom_code_handler._PROMPT_LOADER",
        _FakePromptLoader(),
    )

    result = asyncio.run(
        execute_batch_custom_code_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="batch.custom_code.plan",
                step_id="single-custom-code-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "SingleEffectPatch",
                    "description": "补一个单资产 custom_code 示例",
                    "image_mode": "ai",
                },
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                ),
            ),
            text_step_executor=fake_text_step,
        )
    )

    assert captured["variables"]["item_name"] == "SingleEffectPatch"
    assert result["item_name"] == "SingleEffectPatch"
    assert result["text"] == "先补单资产 custom_code 的核心类骨架"
