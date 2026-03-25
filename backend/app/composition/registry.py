from __future__ import annotations

from collections import defaultdict
from typing import Any


class ProviderRegistry:
    def __init__(self) -> None:
        self._providers: dict[str, dict[str, Any]] = defaultdict(dict)

    def register(self, kind: str, name: str, provider: Any) -> None:
        self._providers[kind][name] = provider

    def get(self, kind: str, name: str) -> Any:
        return self._providers[kind][name]

    def has(self, kind: str, name: str) -> bool:
        return name in self._providers.get(kind, {})
