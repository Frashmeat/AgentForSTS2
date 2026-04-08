"""Tests for agent runner backend resolution."""
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.prompting import PromptLoader
from llm import prompt_builder
from llm.agent_runner import build_agent_prompt, resolve_agent_backend


def test_agent_runner_selects_claude_backend():
    llm_cfg = {"mode": "agent_cli", "agent_backend": "claude"}
    assert resolve_agent_backend(llm_cfg) == "claude"


def test_agent_runner_selects_codex_backend():
    llm_cfg = {"mode": "agent_cli", "agent_backend": "codex"}
    assert resolve_agent_backend(llm_cfg) == "codex"


def test_agent_runner_selects_claude_backend_for_anthropic_api_mode():
    llm_cfg = {"mode": "api", "provider": "anthropic"}
    assert resolve_agent_backend(llm_cfg) == "claude"


def test_agent_runner_selects_codex_backend_for_openai_compatible_api_mode():
    llm_cfg = {"mode": "api", "provider": "qwen"}
    assert resolve_agent_backend(llm_cfg) == "codex"


def test_build_agent_prompt_appends_custom_prompt():
    llm_cfg = {"custom_prompt": "prefer minimal edits"}
    prompt = build_agent_prompt("fix the project", llm_cfg)
    assert "fix the project" in prompt
    assert "prefer minimal edits" in prompt
    assert "User Configured Global AI Instructions" in prompt


def test_build_agent_prompt_keeps_original_when_custom_prompt_blank():
    assert build_agent_prompt("fix the project", {"custom_prompt": ""}) == "fix the project"


def test_build_agent_prompt_uses_latest_runtime_custom_prompt_when_requested(monkeypatch):
    from llm import agent_runner

    monkeypatch.setattr(agent_runner, "get_config", lambda: {"llm": {"custom_prompt": ""}})
    assert agent_runner.build_agent_prompt(
        "fix the project",
        {"custom_prompt": "stale prompt"},
        use_runtime_config=True,
    ) == "fix the project"


def test_build_agent_prompt_injects_agents_codex_for_codex_backend(monkeypatch, tmp_path):
    from llm import agent_runner

    agents_file = tmp_path / "AGENTS_CODEX.md"
    agents_file.write_text("codex rules", encoding="utf-8")
    monkeypatch.setattr(agent_runner, "_AGENTS_CODEX_PATH", agents_file)

    prompt = agent_runner.build_agent_prompt(
        "fix the project",
        {"agent_backend": "codex", "custom_prompt": ""},
    )

    assert prompt.startswith("codex rules\n\n---\n\nfix the project")


def test_build_agent_prompt_skips_agents_codex_for_non_codex_backend(monkeypatch, tmp_path):
    from llm import agent_runner

    agents_file = tmp_path / "AGENTS_CODEX.md"
    agents_file.write_text("codex rules", encoding="utf-8")
    monkeypatch.setattr(agent_runner, "_AGENTS_CODEX_PATH", agents_file)

    prompt = agent_runner.build_agent_prompt(
        "fix the project",
        {"agent_backend": "claude", "custom_prompt": ""},
    )

    assert prompt == "fix the project"


def test_build_agent_prompt_injects_agents_codex_for_api_mode_resolved_to_codex(monkeypatch, tmp_path):
    from llm import agent_runner

    agents_file = tmp_path / "AGENTS_CODEX.md"
    agents_file.write_text("codex rules", encoding="utf-8")
    monkeypatch.setattr(agent_runner, "_AGENTS_CODEX_PATH", agents_file)

    prompt = agent_runner.build_agent_prompt(
        "fix the project",
        {"mode": "api", "provider": "openai", "custom_prompt": ""},
    )

    assert prompt.startswith("codex rules\n\n---\n\nfix the project")


def test_build_agent_prompt_uses_shared_bundle_header_when_legacy_path_missing(monkeypatch):
    prompt = build_agent_prompt("fix the project", {"custom_prompt": "prefer minimal edits"})
    expected_header = PromptLoader().load("runtime_agent.llm_global_prompt_header").strip()

    assert prompt == f"fix the project\n\n{expected_header}\nprefer minimal edits"
