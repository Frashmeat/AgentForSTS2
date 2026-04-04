from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.application.services.config_facade_service import ConfigFacadeService
from routers import config_router


def test_config_facade_returns_boolean_capability_flags(monkeypatch):
    monkeypatch.setattr(
        "app.modules.platform.application.services.config_facade_service.get_config",
        lambda: {
            "llm": {
                "mode": "agent_cli",
                "agent_backend": "codex",
            },
            "image_gen": {
                "mode": "cloud",
                "provider": "volcengine",
                "model": "seedream",
                "api_key": "img-key",
                "api_secret": "img-secret",
            },
        },
    )

    result = ConfigFacadeService().get_local_ai_capability_status()

    assert result == {
        "text_ai_available": True,
        "image_ai_available": True,
    }


def test_config_router_only_exposes_boolean_capability_status(monkeypatch):
    monkeypatch.setattr(
        config_router,
        "get_config",
        lambda: {
            "llm": {
                "mode": "api",
                "provider": "openai",
                "model": "gpt-5.4",
                "api_key": "secret-key",
            },
            "image_gen": {
                "mode": "cloud",
                "provider": "bfl",
                "model": "flux.2-flex",
                "api_key": "",
            },
        },
    )

    result = config_router.local_ai_capability_status()

    assert result == {
        "text_ai_available": True,
        "image_ai_available": False,
    }
    assert "api_key" not in result
