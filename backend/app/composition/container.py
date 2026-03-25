from __future__ import annotations

from typing import Any, Optional

from app.shared.infra.config.settings import Settings

from .registry import ProviderRegistry


class ApplicationContainer:
    def __init__(self, settings: Settings, registry: Optional[ProviderRegistry] = None) -> None:
        self.settings = settings
        self.registry = registry or ProviderRegistry()
        self._singletons: dict[str, Any] = {}

    @classmethod
    def from_config(cls, config: Optional[dict[str, Any]]) -> "ApplicationContainer":
        return cls(settings=Settings.from_dict(config))

    def register_singleton(self, key: str, instance: Any) -> None:
        self._singletons[key] = instance

    def resolve_singleton(self, key: str) -> Any:
        return self._singletons[key]

    def register_provider(self, kind: str, name: str, provider: Any) -> None:
        self.registry.register(kind, name, provider)

    def resolve_provider(self, kind: str, name: str) -> Any:
        return self.registry.get(kind, name)
