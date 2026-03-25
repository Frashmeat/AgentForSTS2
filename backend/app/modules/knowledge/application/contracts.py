from __future__ import annotations

from typing import Protocol


class KnowledgeSource(Protocol):
    def load_context(self, context_type: str, asset_type: str | None = None) -> str: ...
