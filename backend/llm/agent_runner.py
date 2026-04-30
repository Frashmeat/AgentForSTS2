from __future__ import annotations

import logging
from pathlib import Path

from app.shared.infra.llm.agent_backend import (
    AgentBackendRegistry,
    FunctionAgentBackend,
    resolve_agent_backend_name,
)
from app.shared.infra.llm.agent_backend import (
    AgentRunner as PortAgentRunner,
)
from config import get_config, normalize_llm_config
from llm.agent_backends import run_claude_cli, run_codex_cli
from llm.prompt_builder import append_global_ai_instructions

logger = logging.getLogger(__name__)


def resolve_agent_backend(llm_cfg: dict) -> str:
    cfg = normalize_llm_config(llm_cfg)
    return resolve_agent_backend_name(cfg)


def _build_default_registry() -> AgentBackendRegistry:
    registry = AgentBackendRegistry()
    registry.register("claude", FunctionAgentBackend(stream_fn=run_claude_cli))
    registry.register("codex", FunctionAgentBackend(stream_fn=run_codex_cli))
    return registry


def _with_latest_runtime_custom_prompt(llm_cfg: dict) -> dict:
    runtime_llm_cfg = normalize_llm_config(get_config().get("llm"))
    merged = normalize_llm_config(llm_cfg)
    merged["custom_prompt"] = runtime_llm_cfg.get("custom_prompt", "")
    return merged


def build_agent_prompt(prompt: str, llm_cfg: dict, use_runtime_config: bool = False) -> str:
    effective_cfg = _with_latest_runtime_custom_prompt(llm_cfg) if use_runtime_config else llm_cfg
    return append_global_ai_instructions(prompt, effective_cfg)


class AgentRunner(PortAgentRunner):
    pass


async def run_agent_task_with_llm_config(
    prompt: str,
    project_root: Path,
    llm_cfg: dict,
    stream_callback=None,
    *,
    use_runtime_custom_prompt: bool = True,
) -> str:
    effective_llm_cfg = normalize_llm_config(llm_cfg)
    backend = resolve_agent_backend(effective_llm_cfg)
    logger.info("run_agent_task backend=%s project=%s prompt_len=%d", backend, project_root, len(prompt))
    prompt = build_agent_prompt(prompt, effective_llm_cfg, use_runtime_config=use_runtime_custom_prompt)
    runner = AgentRunner(registry=_build_default_registry())
    return await runner.run(prompt, project_root, effective_llm_cfg, stream_callback)


async def run_agent_task(prompt: str, project_root: Path, stream_callback=None) -> str:
    return await run_agent_task_with_llm_config(
        prompt,
        project_root,
        get_config()["llm"],
        stream_callback,
        use_runtime_custom_prompt=True,
    )
