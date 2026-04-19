from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.build_project_handler import execute_build_project_step


def test_execute_build_project_step_runs_agent_with_execution_binding(tmp_path):
    captured: dict[str, object] = {}
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    output_dir = workspace_root / "bin" / "Release"
    output_dir.mkdir(parents=True)
    (output_dir / "DarkMod.dll").write_bytes(b"dll")
    (output_dir / "DarkMod.pck").write_bytes(b"pck")

    def fake_prompt_builder(max_attempts: int) -> str:
        captured["max_attempts"] = max_attempts
        return f"prompt:build:{max_attempts}"

    async def fake_build_agent_runner(prompt, project_root, llm_cfg):
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        captured["llm_cfg"] = dict(llm_cfg)
        return "Summary: 已完成 BattleScriptManager 的服务器项目构建\nBuild succeeded"

    result = asyncio.run(
        execute_build_project_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="build.project",
                step_id="batch.custom_code.build",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "BattleScriptManager",
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
            build_agent_runner=fake_build_agent_runner,
        )
    )

    assert captured["max_attempts"] == 3
    assert captured["prompt"] == "prompt:build:3"
    assert captured["project_root"] == workspace_root
    assert captured["llm_cfg"]["agent_backend"] == "codex"
    assert result["text"] == "已完成 BattleScriptManager 的服务器项目构建"
    assert result["item_name"] == "BattleScriptManager"
    assert [artifact["file_name"] for artifact in result["artifacts"]] == ["DarkMod.dll", "DarkMod.pck"]


def test_execute_build_project_step_requires_server_workspace_root():
    try:
        asyncio.run(
            execute_build_project_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="build.project",
                    step_id="batch.custom_code.build",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"item_name": "BattleScriptManager"},
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
        assert str(error) == "build.project requires server_workspace_root"
    else:
        raise AssertionError("expected ValueError when server_workspace_root is missing")
