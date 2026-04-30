from __future__ import annotations

from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Protocol

StreamCallback = Callable[[str], Awaitable[None]]


class AgentBackend(Protocol):
    async def plan(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: StreamCallback | None = None,
    ) -> str: ...

    async def stream(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: StreamCallback | None = None,
    ) -> str: ...


class TextBackend(Protocol):
    async def complete(
        self,
        prompt: str,
        llm_cfg: dict,
        cwd: Path | None = None,
    ) -> str: ...

    async def stream(
        self,
        system_prompt: str,
        user_prompt: str,
        llm_cfg: dict,
        on_chunk: StreamCallback,
        cwd: Path | None = None,
    ) -> str: ...
