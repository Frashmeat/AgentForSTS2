from __future__ import annotations

from pathlib import Path
from typing import Optional

from .contracts import AgentBackend, StreamCallback


_CLI_AGENT_BACKENDS = {"claude", "codex"}


def resolve_api_agent_backend_name(provider: str) -> str:
    normalized = str(provider).strip().lower()
    if normalized == "anthropic":
        return "claude"
    return "codex"


def resolve_agent_backend_name(llm_cfg: dict) -> str:
    mode = str(llm_cfg.get("mode", "")).strip()
    if mode == "api":
        return resolve_api_agent_backend_name(llm_cfg.get("provider", ""))

    backend = str(llm_cfg.get("agent_backend", "claude")).strip().lower()
    if backend in _CLI_AGENT_BACKENDS:
        return backend
    return "claude"


class FunctionAgentBackend:
    def __init__(self, stream_fn, plan_fn=None) -> None:
        self._stream_fn = stream_fn
        self._plan_fn = plan_fn or stream_fn

    async def plan(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: Optional[StreamCallback] = None,
    ) -> str:
        return await self._plan_fn(prompt, project_root, llm_cfg, stream_callback)

    async def stream(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: Optional[StreamCallback] = None,
    ) -> str:
        return await self._stream_fn(prompt, project_root, llm_cfg, stream_callback)


class AgentBackendRegistry:
    def __init__(self) -> None:
        self._backends: dict[str, AgentBackend] = {}

    def register(self, name: str, backend: AgentBackend) -> None:
        self._backends[name] = backend

    def get(self, name: str) -> AgentBackend:
        return self._backends[name]


class AgentRunner:
    def __init__(self, backend: AgentBackend | None = None, registry: AgentBackendRegistry | None = None) -> None:
        self.backend = backend
        self.registry = registry or AgentBackendRegistry()

    def _resolve_backend(self, llm_cfg: dict) -> AgentBackend:
        if self.backend is not None:
            return self.backend
        return self.registry.get(resolve_agent_backend_name(llm_cfg))

    async def run(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: Optional[StreamCallback] = None,
    ) -> str:
        return await self._resolve_backend(llm_cfg).stream(prompt, project_root, llm_cfg, stream_callback)
