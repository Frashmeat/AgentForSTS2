from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.code_generate_handler import (
    build_code_llm_config,
    execute_code_generate_step,
)


def test_build_code_llm_config_uses_execution_binding():
    llm_cfg = build_code_llm_config(
        StepExecutionBinding(
            agent_backend="codex",
            provider="openai",
            model="gpt-5.4",
            credential="sk-live-openai",
            base_url="https://api.openai.com/v1",
        )
    )

    assert llm_cfg["mode"] == "agent_cli"
    assert llm_cfg["agent_backend"] == "codex"
    assert llm_cfg["model"] == "gpt-5.4"
    assert llm_cfg["api_key"] == "sk-live-openai"
    assert llm_cfg["base_url"] == "https://api.openai.com/v1"


def test_execute_code_generate_step_builds_prompt_and_runs_agent(monkeypatch, tmp_path):
    captured: dict[str, object] = {}
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    (workspace_root / "MainFile.cs").write_text("namespace DarkMod;", encoding="utf-8")

    def fake_prompt_builder(request):
        captured["prompt_request"] = request
        return f"prompt:{request.name}|{request.project_root.name}"

    async def fake_code_agent_runner(prompt, project_root, llm_cfg):
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        captured["llm_cfg"] = dict(llm_cfg)
        return "Summary: 已写入 SingleEffectPatch 的服务器 custom_code 代码\nDetailed output"

    result = asyncio.run(
        execute_code_generate_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="code.generate",
                step_id="single.custom_code.codegen",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "SingleEffectPatch",
                    "description": "补一个单资产 custom_code 示例",
                    "analysis": "摘要：建议先补一个 Harmony Patch 骨架",
                    "server_workspace_root": str(workspace_root),
                },
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                    base_url="https://api.openai.com/v1",
                ),
            ),
            prompt_builder=fake_prompt_builder,
            code_agent_runner=fake_code_agent_runner,
        )
    )

    prompt_request = captured["prompt_request"]
    assert prompt_request.name == "SingleEffectPatch"
    assert prompt_request.project_root == workspace_root
    assert prompt_request.skip_build is True
    assert prompt_request.implementation_notes == "摘要：建议先补一个 Harmony Patch 骨架"
    assert captured["prompt"] == "prompt:SingleEffectPatch|DarkMod"
    assert captured["project_root"] == workspace_root
    assert captured["llm_cfg"]["agent_backend"] == "codex"
    assert result["text"] == "已写入 SingleEffectPatch 的服务器 custom_code 代码"
    assert result["item_name"] == "SingleEffectPatch"
    assert result["server_workspace_root"] == str(workspace_root)
    assert result["analysis"].startswith("Summary:")


def test_execute_code_generate_step_requires_server_workspace_root():
    try:
        asyncio.run(
            execute_code_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="code.generate",
                    step_id="single.custom_code.codegen",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={
                        "item_name": "SingleEffectPatch",
                        "description": "补一个单资产 custom_code 示例",
                        "analysis": "摘要：建议先补一个 Harmony Patch 骨架",
                    },
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
        assert str(error) == "code.generate requires server_workspace_root"
    else:
        raise AssertionError("expected ValueError when server_workspace_root is missing")
