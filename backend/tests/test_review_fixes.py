"""Regression tests for review findings fixed on 2026-03-24."""

import asyncio
import sys
from pathlib import Path
from types import ModuleType

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

if "litellm" not in sys.modules:
    litellm_stub = ModuleType("litellm")

    async def _unexpected_acompletion(*_args, **_kwargs):
        raise AssertionError("litellm should not be called in review_fixes tests")

    litellm_stub.acompletion = _unexpected_acompletion
    sys.modules["litellm"] = litellm_stub

from app.modules.codegen import api as codegen_api
from routers import batch_workflow


def test_create_asset_prompt_uses_mod_localization_root(tmp_path, monkeypatch):
    captured: dict[str, str] = {}

    async def fake_run(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        return "ok"

    monkeypatch.setattr(codegen_api, "run_claude_code", fake_run)

    project_root = tmp_path / "SampleMod"
    project_root.mkdir()

    asyncio.run(
        codegen_api.create_asset(
            design_description="测试描述",
            asset_type="card",
            asset_name="DarkBlade",
            image_paths=[],
            project_root=project_root,
        )
    )

    prompt = captured["prompt"]
    assert f"{project_root.name}/localization/eng/<type>s.json" in prompt
    assert f"{project_root.name}/localization/zhs/<type>s.json" in prompt
    assert "DarkBlade/localization/eng/<type>s.json" not in prompt
    assert "DarkBlade/localization/zhs/<type>s.json" not in prompt


def test_routers_import_module_entrypoints_instead_of_agents():
    batch_source = Path(batch_workflow.__file__).read_text(encoding="utf-8")
    workflow_source = Path(Path(batch_workflow.__file__).parent / "workflow.py").read_text(encoding="utf-8")
    build_source = Path(Path(batch_workflow.__file__).parent / "build_deploy.py").read_text(encoding="utf-8")

    assert "from agents.code_agent import" not in batch_source
    assert "from agents.planner import" not in batch_source
    assert "from agents.code_agent import" not in workflow_source
    assert "from agents.code_agent import" not in build_source


def test_batch_workflow_retry_path_does_not_call_missing_process_item():
    source = Path(batch_workflow.__file__).read_text(encoding="utf-8")
    assert "process_item(" not in source
