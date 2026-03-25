from __future__ import annotations

import logging
from pathlib import Path

from app.shared.infra.llm.agent_backend import (
    AgentBackendRegistry,
    AgentRunner as PortAgentRunner,
    FunctionAgentBackend,
    resolve_agent_backend_name,
)
from config import get_config, normalize_llm_config
from llm.agent_backends import run_claude_cli, run_codex_cli
from llm.prompt_builder import append_global_ai_instructions

logger = logging.getLogger(__name__)

_AGENTS_CODEX_PATH = Path(__file__).parent.parent.parent / "AGENTS_CODEX.md"


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


def _inject_agents_codex(prompt: str) -> str:
    if not _AGENTS_CODEX_PATH.exists():
        return prompt
    agents_codex = _AGENTS_CODEX_PATH.read_text(encoding="utf-8").strip()
    return f"{agents_codex}\n\n---\n\n{prompt}"


def build_agent_prompt(prompt: str, llm_cfg: dict, use_runtime_config: bool = False) -> str:
    effective_cfg = _with_latest_runtime_custom_prompt(llm_cfg) if use_runtime_config else llm_cfg
    backend = normalize_llm_config(effective_cfg).get("agent_backend", "claude")
    if backend == "codex":
        prompt = _inject_agents_codex(prompt)
    return append_global_ai_instructions(prompt, effective_cfg)


class AgentRunner(PortAgentRunner):
    pass


async def run_agent_task(prompt: str, project_root: Path, stream_callback=None) -> str:
    llm_cfg = get_config()["llm"]
    backend = resolve_agent_backend(llm_cfg)
    logger.info("run_agent_task backend=%s project=%s prompt_len=%d", backend, project_root, len(prompt))
    prompt = build_agent_prompt(prompt, llm_cfg, use_runtime_config=True)
    runner = AgentRunner(registry=_build_default_registry())
    return await runner.run(prompt, project_root, llm_cfg, stream_callback)
