from __future__ import annotations

from pathlib import Path

from .contracts import StreamCallback, TextBackend


def resolve_text_backend_name(llm_cfg: dict) -> str:
    if llm_cfg.get("mode") == "agent_cli":
        return f"{llm_cfg.get('agent_backend', 'claude')}_cli"
    return "litellm"


class FunctionTextBackend:
    def __init__(self, complete_fn, stream_fn) -> None:
        self._complete_fn = complete_fn
        self._stream_fn = stream_fn

    async def complete(
        self,
        prompt: str,
        llm_cfg: dict,
        cwd: Path | None = None,
    ) -> str:
        return await self._complete_fn(prompt, llm_cfg, cwd)

    async def stream(
        self,
        system_prompt: str,
        user_prompt: str,
        llm_cfg: dict,
        on_chunk: StreamCallback,
        cwd: Path | None = None,
    ) -> str:
        return await self._stream_fn(system_prompt, user_prompt, llm_cfg, on_chunk, cwd)


class TextBackendRegistry:
    def __init__(self) -> None:
        self._backends: dict[str, TextBackend] = {}

    def register(self, name: str, backend: TextBackend) -> None:
        self._backends[name] = backend

    def get(self, name: str) -> TextBackend:
        return self._backends[name]


class TextRunner:
    def __init__(self, backend: TextBackend | None = None, registry: TextBackendRegistry | None = None) -> None:
        self.backend = backend
        self.registry = registry or TextBackendRegistry()

    def _resolve_backend(self, llm_cfg: dict) -> TextBackend:
        if self.backend is not None:
            return self.backend
        return self.registry.get(resolve_text_backend_name(llm_cfg))

    async def complete(
        self,
        prompt: str,
        llm_cfg: dict,
        cwd: Path | None = None,
    ) -> str:
        return await self._resolve_backend(llm_cfg).complete(prompt, llm_cfg, cwd)

    async def stream(
        self,
        system_prompt: str,
        user_prompt: str,
        llm_cfg: dict,
        on_chunk: StreamCallback,
        cwd: Path | None = None,
    ) -> str:
        return await self._resolve_backend(llm_cfg).stream(system_prompt, user_prompt, llm_cfg, on_chunk, cwd)
