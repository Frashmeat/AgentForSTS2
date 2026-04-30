from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass(slots=True)
class ImageGenerationRequest:
    provider: str
    prompt: str
    asset_type: str
    batch_size: int = 1
    negative_prompt: str | None = None
    options: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class ImagePostProcessRequest:
    asset_type: str
    name: str
    project_root: Path
