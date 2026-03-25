import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.composition.container import ApplicationContainer
from app.composition.registry import ProviderRegistry
from app.shared.infra.config.settings import Settings, normalize_config


def test_registry_resolves_named_provider():
    registry = ProviderRegistry()
    provider = object()

    registry.register("image", "mock", provider)

    assert registry.get("image", "mock") is provider


def test_container_builds_settings_and_resolves_singletons():
    container = ApplicationContainer.from_config({"llm": {"agent_backend": "codex"}})
    service = object()

    container.register_singleton("workflow", service)

    resolved_settings = container.settings
    resolved_service = container.resolve_singleton("workflow")

    assert isinstance(resolved_settings, Settings)
    assert resolved_settings.llm["agent_backend"] == "codex"
    assert resolved_service is service


def test_normalize_config_keeps_legacy_defaults():
    cfg = normalize_config(None)

    assert cfg["llm"]["execution_mode"] == "legacy_direct"
