from __future__ import annotations

from pathlib import Path
from typing import Awaitable, Callable, Optional, Protocol


StreamCallback = Callable[[str], Awaitable[None]]


class AgentBackend(Protocol):
    async def plan(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: Optional[StreamCallback] = None,
    ) -> str: ...

    async def stream(
        self,
        prompt: str,
        project_root: Path,
        llm_cfg: dict,
        stream_callback: Optional[StreamCallback] = None,
    ) -> str: ...


class TextBackend(Protocol):
    async def complete(
        self,
        prompt: str,
        llm_cfg: dict,
        cwd: Optional[Path] = None,
    ) -> str: ...

    async def stream(
        self,
        system_prompt: str,
        user_prompt: str,
        llm_cfg: dict,
        on_chunk: StreamCallback,
        cwd: Optional[Path] = None,
    ) -> str: ...
