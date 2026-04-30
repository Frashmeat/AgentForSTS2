"""Tests for LLM config normalization and old input cleanup."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from config import normalize_config, normalize_llm_config


def test_normalize_old_claude_subscription_config():
    cfg = normalize_config({"llm": {"mode": "claude_subscription"}})
    assert cfg["llm"]["mode"] == "agent_cli"
    assert cfg["llm"]["agent_backend"] == "claude"


def test_normalize_old_api_key_config():
    cfg = normalize_config({"llm": {"mode": "api_key", "provider": "anthropic"}})
    assert cfg["llm"]["mode"] == "claude_api"
    assert cfg["llm"]["provider"] == "anthropic"


def test_normalize_frontend_agent_cli_codex_payload():
    llm_cfg = normalize_llm_config({"mode": "agent_cli", "agent_backend": "codex"})
    assert llm_cfg["mode"] == "agent_cli"
    assert llm_cfg["agent_backend"] == "codex"


def test_normalize_frontend_claude_api_payload():
    llm_cfg = normalize_llm_config({"mode": "claude_api", "provider": " OpenAI ", "model": "claude-sonnet-4-6"})
    assert llm_cfg["mode"] == "claude_api"
    assert llm_cfg["provider"] == "openai"
    assert llm_cfg["model"] == "claude-sonnet-4-6"


def test_normalize_llm_config_sets_default_custom_prompt():
    llm_cfg = normalize_llm_config({})
    assert llm_cfg["custom_prompt"] == ""
