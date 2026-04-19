from __future__ import annotations

import asyncio
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.asset_generate_handler import execute_asset_generate_step


def test_execute_asset_generate_step_runs_postprocess_and_agent(tmp_path):
    captured: dict[str, object] = {}
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    uploaded_asset_path = tmp_path / "uploaded.png"
    Image.new("RGBA", (16, 16), (255, 0, 0, 255)).save(uploaded_asset_path)

    async def fake_postprocess(*, uploaded_asset_path, asset_type, item_name, project_root):
        captured["postprocess"] = (uploaded_asset_path, asset_type, item_name, project_root)
        generated = project_root / project_root.name / "images" / "card_portraits" / f"{item_name}.png"
        generated.parent.mkdir(parents=True, exist_ok=True)
        generated.write_bytes(b"png")
        return [generated]

    def fake_prompt_builder(request):
        captured["prompt_request"] = request
        return f"prompt:{request.asset_type}|{request.asset_name}|{len(request.image_paths)}"

    async def fake_asset_agent_runner(prompt, project_root, llm_cfg):
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        captured["llm_cfg"] = dict(llm_cfg)
        return "Summary: 已写入 DarkBladeFullscreen 的服务器资产代码\nDone"

    import app.modules.platform.runner.asset_generate_handler as module

    original = module._run_postprocess_in_worker
    module._run_postprocess_in_worker = fake_postprocess
    try:
        result = asyncio.run(
            execute_asset_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="asset.generate",
                    step_id="single.card_fullscreen.asset",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={
                        "asset_type": "card_fullscreen",
                        "item_name": "DarkBladeFullscreen",
                        "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                        "server_workspace_root": str(workspace_root),
                        "uploaded_asset_path": str(uploaded_asset_path),
                    },
                    execution_binding=StepExecutionBinding(
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                prompt_builder=fake_prompt_builder,
                asset_agent_runner=fake_asset_agent_runner,
            )
        )
    finally:
        module._run_postprocess_in_worker = original

    assert captured["postprocess"][1:] == ("card_fullscreen", "DarkBladeFullscreen", workspace_root)
    prompt_request = captured["prompt_request"]
    assert prompt_request.asset_type == "card_fullscreen"
    assert prompt_request.asset_name == "DarkBladeFullscreen"
    assert captured["prompt"] == "prompt:card_fullscreen|DarkBladeFullscreen|1"
    assert captured["llm_cfg"]["agent_backend"] == "codex"
    assert result["text"] == "已写入 DarkBladeFullscreen 的服务器资产代码"
    assert result["generated_image_paths"][0].endswith("DarkBladeFullscreen.png")


def test_execute_asset_generate_step_requires_uploaded_asset_path():
    try:
        asyncio.run(
            execute_asset_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="asset.generate",
                    step_id="single.card_fullscreen.asset",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={
                        "asset_type": "card_fullscreen",
                        "item_name": "DarkBladeFullscreen",
                        "description": "一张强调暗影剑士出招姿态的全画面卡插图方案。",
                        "server_workspace_root": "F:/runtime/platform-workspaces/1001/abc123/DarkMod",
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
        assert str(error) == "asset.generate requires uploaded_asset_path"
    else:
        raise AssertionError("expected ValueError when uploaded_asset_path is missing")
