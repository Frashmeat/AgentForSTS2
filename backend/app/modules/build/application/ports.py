from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol


@dataclass
class BuildRequest:
    command: list[str]
    cwd: Path


@dataclass
class BuildResult:
    success: bool
    output: str = ""
    metadata: dict[str, Any] = field(default_factory=dict)


class BuildBackend(Protocol):
    async def build(self, request: BuildRequest) -> BuildResult: ...
