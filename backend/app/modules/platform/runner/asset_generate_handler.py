from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
from pathlib import Path

from PIL import Image

from app.modules.codegen.api import build_asset_prompt
from app.modules.codegen.domain.models import AssetCodegenRequest
from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from image.postprocess import process_image
from llm.agent_runner import run_agent_task_with_llm_config

from .code_generate_handler import build_code_llm_config

AssetAgentRunner = Callable[[str, Path, dict[str, object]], Awaitable[str]]


def _resolve_required_text(input_payload: dict[str, object], key: str) -> str:
    value = str(input_payload.get(key, "")).strip()
    if not value:
        raise ValueError(f"asset.generate requires {key}")
    return value


def _load_uploaded_image(uploaded_asset_path: Path) -> Image.Image:
    return Image.open(uploaded_asset_path).convert("RGBA")


async def _run_postprocess_in_worker(
    *,
    uploaded_asset_path: Path,
    asset_type: str,
    item_name: str,
    project_root: Path,
) -> list[Path]:
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(
        None,
        lambda: process_image(_load_uploaded_image(uploaded_asset_path), asset_type, item_name, project_root),
    )


def _build_summary(full_text: str, item_name: str) -> str:
    for raw_line in full_text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.lower().startswith("summary:"):
            summary = line.split(":", 1)[1].strip()
            if summary:
                return summary[:120]
        if line.startswith("摘要："):
            summary = line[3:].strip()
            if summary:
                return summary[:120]
    return f"已写入 {item_name} 的服务器资产代码"


async def execute_asset_generate_step(
    request: StepExecutionRequest,
    *,
    prompt_builder: Callable[[AssetCodegenRequest], str] = build_asset_prompt,
    asset_agent_runner: AssetAgentRunner = run_agent_task_with_llm_config,
) -> dict[str, object]:
    input_payload = request.input_payload
    asset_type = _resolve_required_text(input_payload, "asset_type")
    item_name = _resolve_required_text(input_payload, "item_name")
    description = _resolve_required_text(input_payload, "description")
    project_root = Path(_resolve_required_text(input_payload, "server_workspace_root"))
    uploaded_asset_path = Path(_resolve_required_text(input_payload, "uploaded_asset_path"))

    image_paths = await _run_postprocess_in_worker(
        uploaded_asset_path=uploaded_asset_path,
        asset_type=asset_type,
        item_name=item_name,
        project_root=project_root,
    )
    prompt = prompt_builder(
        AssetCodegenRequest(
            design_description=description,
            asset_type=asset_type,
            asset_name=item_name,
            image_paths=image_paths,
            project_root=project_root,
            skip_build=True,
        )
    )
    llm_cfg = build_code_llm_config(request.execution_binding)
    full_text = await asset_agent_runner(prompt, project_root, llm_cfg)
    return {
        "text": _build_summary(full_text, item_name),
        "analysis": full_text,
        "asset_type": asset_type,
        "item_name": item_name,
        "server_workspace_root": str(project_root),
        "generated_image_paths": [str(path) for path in image_paths],
    }
