from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.application.services.config_facade_service import ConfigFacadeService
from routers import config_router


def test_config_facade_returns_capability_flags_with_empty_reasons_when_available(monkeypatch):
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
        "code_agent_available": True,
        "image_ai_available": True,
        "text_ai_missing_reasons": [],
        "code_agent_missing_reasons": [],
        "image_ai_missing_reasons": [],
    }


def test_config_router_exposes_capability_reasons_without_secret_fields(monkeypatch):
    monkeypatch.setattr(
        config_router,
        "get_config",
        lambda: {
            "llm": {
                "mode": "claude_api",
                "model": "claude-sonnet-4-6",
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
        "code_agent_available": True,
        "image_ai_available": False,
        "text_ai_missing_reasons": [],
        "code_agent_missing_reasons": [],
        "image_ai_missing_reasons": ["请先在设置中填写图像 API Key。"],
    }
    assert "api_key" not in result


def test_config_router_marks_code_agent_available_for_claude_api_mode(monkeypatch):
    monkeypatch.setattr(
        config_router,
        "get_config",
        lambda: {
            "llm": {
                "mode": "claude_api",
                "model": "claude-sonnet-4-6",
                "api_key": "secret-key",
            },
            "image_gen": {
                "mode": "cloud",
                "provider": "bfl",
                "model": "flux.2-flex",
                "api_key": "img-key",
            },
        },
    )

    result = config_router.local_ai_capability_status()

    assert result["text_ai_available"] is True
    assert result["code_agent_available"] is True
    assert result["code_agent_missing_reasons"] == []
