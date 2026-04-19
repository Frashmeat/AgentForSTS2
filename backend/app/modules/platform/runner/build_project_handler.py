from __future__ import annotations

from collections.abc import Awaitable, Callable
import mimetypes
from pathlib import Path

from app.modules.codegen.api import build_codegen_prompt_assembler
from app.modules.platform.infra.build_output_files import deploy_latest_output_files, find_latest_output_files
from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.shared.prompting import PromptLoader
from config import get_config
from llm.agent_runner import run_agent_task_with_llm_config
from project_utils import ensure_local_props

from .code_generate_handler import build_code_llm_config


BuildAgentRunner = Callable[[str, Path, dict[str, object]], Awaitable[str]]

_TEXT_LOADER = PromptLoader()


def _resolve_project_root(input_payload: dict[str, object]) -> Path:
    root_text = str(input_payload.get("server_workspace_root", "")).strip()
    if not root_text:
        raise ValueError("build.project requires server_workspace_root")
    project_root = Path(root_text)
    if not project_root.exists():
        raise ValueError(f"server workspace root does not exist: {project_root}")
    return project_root


def _resolve_item_name(input_payload: dict[str, object]) -> str:
    return str(input_payload.get("item_name", "")).strip() or "custom_code"


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
    return _TEXT_LOADER.render(
        "runtime_workflow.build_build_succeeded",
        {},
    ).strip() or f"已完成 {item_name} 的服务器项目构建"


def _build_deploy_summary(item_name: str, deployed_to: str) -> str:
    return f"已完成 {item_name} 的服务器构建并部署到 {deployed_to}"


def _build_artifact_payloads(project_root: Path) -> list[dict[str, object]]:
    payloads: list[dict[str, object]] = []
    for file in find_latest_output_files(project_root):
        mime_type, _ = mimetypes.guess_type(file.name)
        payloads.append(
            {
                "artifact_type": "build_output",
                "storage_provider": "server_workspace",
                "object_key": str(file),
                "file_name": file.name,
                "mime_type": mime_type or "application/octet-stream",
                "size_bytes": file.stat().st_size,
                "result_summary": "服务器构建产物",
            }
        )
    return payloads


def _build_deployed_artifact_payloads(file_paths: list[Path]) -> list[dict[str, object]]:
    payloads: list[dict[str, object]] = []
    for file in file_paths:
        mime_type, _ = mimetypes.guess_type(file.name)
        payloads.append(
            {
                "artifact_type": "deployed_output",
                "storage_provider": "server_deploy",
                "object_key": str(file),
                "file_name": file.name,
                "mime_type": mime_type or "application/octet-stream",
                "size_bytes": file.stat().st_size,
                "result_summary": "服务器部署产物",
            }
        )
    return payloads


async def execute_build_project_step(
    request: StepExecutionRequest,
    *,
    prompt_builder: Callable[[int], str] | None = None,
    build_agent_runner: BuildAgentRunner = run_agent_task_with_llm_config,
    config_loader: Callable[[], dict[str, object]] = get_config,
    ensure_local_props_fn: Callable[[Path], bool] = ensure_local_props,
) -> dict[str, object]:
    project_root = _resolve_project_root(request.input_payload)
    item_name = _resolve_item_name(request.input_payload)
    if prompt_builder is None:
        assembler = build_codegen_prompt_assembler()
        prompt_builder = assembler.assemble_build_prompt

    ensure_local_props_fn(project_root)
    max_attempts = int(request.input_payload.get("build_max_attempts", 3) or 3)
    prompt = prompt_builder(max_attempts)
    llm_cfg = build_code_llm_config(request.execution_binding)
    full_text = await build_agent_runner(prompt, project_root, llm_cfg)
    artifacts = _build_artifact_payloads(project_root)
    deployed_to: str | None = None
    deployed_files: list[str] = []
    sts2_path = str(config_loader().get("sts2_path", "")).strip()
    if sts2_path:
        mods_root = Path(sts2_path) / "Mods"
        if not mods_root.exists():
            raise ValueError(f"server sts2 Mods path does not exist: {mods_root}")
        deployed = deploy_latest_output_files(project_root, mods_root)
        deployed_to = deployed.deployed_to
        deployed_files = list(deployed.file_names)
        artifacts.extend(_build_deployed_artifact_payloads(deployed.file_paths))
    return {
        "text": _build_deploy_summary(item_name, deployed_to) if deployed_to else _build_summary(full_text, item_name),
        "analysis": full_text,
        "item_name": item_name,
        "server_workspace_root": str(project_root),
        "artifacts": artifacts,
        "deployed_to": deployed_to,
        "files": deployed_files,
    }
