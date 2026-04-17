from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.single_asset_plan_handler import execute_single_asset_plan_step


def test_execute_single_asset_plan_step_builds_prompt_and_delegates_to_text_generation(monkeypatch):
    captured: dict[str, object] = {}

    class _FakePromptLoader:
        def render(self, template_name: str, variables: dict[str, object]) -> str:
            captured["template_name"] = template_name
            captured["variables"] = dict(variables)
            return f"prompt:{variables['asset_type']}|{variables['item_name']}"

    async def fake_text_step(request: StepExecutionRequest):
        captured["request"] = request
        return {
            "text": "摘要：建议先补遗物触发与本地化骨架\n- 再补资源路径。",
            "provider": request.execution_binding.provider,
            "model": request.execution_binding.model,
        }

    monkeypatch.setattr(
        "app.modules.platform.runner.single_asset_plan_handler._PROMPT_LOADER",
        _FakePromptLoader(),
    )
    monkeypatch.setattr(
        "app.modules.platform.runner.single_asset_plan_handler.get_docs_for_type",
        lambda asset_type: f"docs:{asset_type}",
    )

    result = asyncio.run(
        execute_single_asset_plan_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="single.asset.plan",
                step_id="single-relic-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "asset_type": "relic",
                    "asset_name": "FangedGrimoire",
                    "description": "每次造成伤害时获得 2 点格挡。",
                    "image_mode": "ai",
                    "has_uploaded_image": False,
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
    assert captured["template_name"] == "runtime_agent.platform_single_asset_server_user"
    assert captured["variables"]["asset_type"] == "relic"
    assert captured["variables"]["item_name"] == "FangedGrimoire"
    assert captured["variables"]["docs"] == "docs:relic"
    assert forwarded.step_type == "text.generate"
    assert forwarded.input_payload["prompt"] == "prompt:relic|FangedGrimoire"
    assert result["asset_type"] == "relic"
    assert result["item_name"] == "FangedGrimoire"
    assert result["text"] == "建议先补遗物触发与本地化骨架"
    assert result["analysis"].startswith("摘要：建议先补遗物触发与本地化骨架")


def test_execute_single_asset_plan_step_requires_description():
    try:
        asyncio.run(
            execute_single_asset_plan_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="single.asset.plan",
                    step_id="single-relic-2",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"asset_type": "relic", "asset_name": "EmptyRelic"},
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
        assert str(error) == "single asset server task requires description"
    else:
        raise AssertionError("expected ValueError when description is missing")


def test_execute_single_asset_plan_step_supports_batch_generate_payload_shape(monkeypatch):
    captured: dict[str, object] = {}

    class _FakePromptLoader:
        def render(self, template_name: str, variables: dict[str, object]) -> str:
            captured["variables"] = dict(variables)
            return f"prompt:{variables['asset_type']}|{variables['item_name']}"

    async def fake_text_step(request: StepExecutionRequest):
        captured["request"] = request
        return {
            "text": "摘要：建议先补卡牌骨架与升级分支",
            "provider": request.execution_binding.provider,
            "model": request.execution_binding.model,
        }

    monkeypatch.setattr(
        "app.modules.platform.runner.single_asset_plan_handler._PROMPT_LOADER",
        _FakePromptLoader(),
    )
    monkeypatch.setattr(
        "app.modules.platform.runner.single_asset_plan_handler.get_docs_for_type",
        lambda asset_type: f"docs:{asset_type}",
    )

    result = asyncio.run(
        execute_single_asset_plan_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="single.asset.plan",
                step_id="batch-card-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "asset_type": "card",
                    "name": "DarkBlade",
                    "description": "1 费攻击牌，造成 8 点伤害，升级后造成 12 点伤害。",
                    "needs_image": True,
                    "has_uploaded_image": False,
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
    assert captured["variables"]["asset_type"] == "card"
    assert captured["variables"]["item_name"] == "DarkBlade"
    assert captured["variables"]["docs"] == "docs:card"
    assert forwarded.input_payload["prompt"] == "prompt:card|DarkBlade"
    assert result["asset_type"] == "card"
    assert result["item_name"] == "DarkBlade"
    assert result["text"] == "建议先补卡牌骨架与升级分支"
