from __future__ import annotations

import asyncio
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.application.services.platform_file_storage import default_platform_storage_root, normalize_object_key
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
                        runner_type="codex_cli",
                        api_protocol="openai_compatible",
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


def test_execute_asset_generate_step_allows_text_only_generation_without_uploaded_asset(tmp_path):
    captured: dict[str, object] = {}
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()

    def fake_prompt_builder(request):
        captured["prompt_request"] = request
        return f"prompt:{request.asset_type}|{request.asset_name}|{len(request.image_paths)}"

    async def fake_asset_agent_runner(prompt, project_root, llm_cfg):
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        return "Summary: 已写入 FangedGrimoire 的服务器资产代码\nDone"

    result = asyncio.run(
        execute_asset_generate_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="asset.generate",
                step_id="single.relic.asset",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "asset_type": "relic",
                    "item_name": "FangedGrimoire",
                    "description": "每次造成伤害时获得 2 点格挡。",
                    "server_workspace_root": str(workspace_root),
                },
                execution_binding=StepExecutionBinding(
                    runner_type="codex_cli",
                    api_protocol="openai_compatible",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                ),
            ),
            prompt_builder=fake_prompt_builder,
            asset_agent_runner=fake_asset_agent_runner,
        )
    )

    prompt_request = captured["prompt_request"]
    assert prompt_request.asset_type == "relic"
    assert prompt_request.asset_name == "FangedGrimoire"
    assert prompt_request.image_paths == []
    assert captured["prompt"] == "prompt:relic|FangedGrimoire|0"
    assert captured["project_root"] == workspace_root
    assert result["text"] == "已写入 FangedGrimoire 的服务器资产代码"
    assert result["generated_image_paths"] == []


def test_execute_asset_generate_step_resolves_platform_object_keys(tmp_path, monkeypatch):
    captured: dict[str, object] = {}
    platform_root = tmp_path / "platform"
    workspace_root = platform_root / "workspaces" / "1001" / "abc123" / "DarkMod"
    workspace_root.mkdir(parents=True)
    upload_path = platform_root / "uploads" / "1001" / "asset123" / "content.png"
    upload_path.parent.mkdir(parents=True)
    Image.new("RGBA", (16, 16), (255, 0, 0, 255)).save(upload_path)
    monkeypatch.setattr(
        "app.modules.platform.application.services.platform_file_storage.default_platform_storage_root",
        lambda: platform_root,
    )

    async def fake_postprocess(*, uploaded_asset_path, asset_type, item_name, project_root):
        captured["postprocess"] = (uploaded_asset_path, asset_type, item_name, project_root)
        return []

    async def fake_asset_agent_runner(prompt, project_root, llm_cfg):
        captured["project_root"] = project_root
        return "Summary: 已写入 FangedGrimoire 的服务器资产代码\nDone"

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
                    step_id="single.relic.asset",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={
                        "asset_type": "relic",
                        "item_name": "FangedGrimoire",
                        "description": "每次造成伤害时获得 2 点格挡。",
                        "server_workspace_object_key": normalize_object_key(
                            "workspaces", 1001, "abc123", "DarkMod"
                        ),
                        "uploaded_asset_object_key": normalize_object_key("uploads", 1001, "asset123", "content.png"),
                    },
                    execution_binding=StepExecutionBinding(
                        runner_type="codex_cli",
                        api_protocol="openai_compatible",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                asset_agent_runner=fake_asset_agent_runner,
            )
        )
    finally:
        module._run_postprocess_in_worker = original

    assert captured["postprocess"][0] == upload_path
    assert captured["postprocess"][3] == workspace_root
    assert captured["project_root"] == workspace_root
    assert result["text"] == "已写入 FangedGrimoire 的服务器资产代码"
