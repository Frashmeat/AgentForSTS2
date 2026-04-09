"""Tests for text runner backend resolution."""
import asyncio
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

if "litellm" not in sys.modules:
    sys.modules["litellm"] = types.SimpleNamespace(acompletion=None)

# test_planning_module injects a stub for llm.text_runner during collection.
# Drop it here so this module imports the real implementation under test.
sys.modules.pop("llm.text_runner", None)

from app.shared.prompting import PromptLoader
from llm import prompt_builder
from llm.text_runner import build_text_prompt, build_system_prompt, resolve_text_backend, resolve_model


def test_text_runner_uses_cli_backend_when_mode_is_agent_cli():
    llm_cfg = {"mode": "agent_cli", "agent_backend": "codex"}
    assert resolve_text_backend(llm_cfg) == "codex_cli"


def test_text_runner_uses_litellm_when_mode_is_claude_api():
    llm_cfg = {"mode": "claude_api", "model": "claude-sonnet-4-6"}
    assert resolve_text_backend(llm_cfg) == "litellm"


def test_resolve_model_falls_back_to_default_claude_model():
    llm_cfg = {"mode": "claude_api"}
    assert resolve_model(llm_cfg) == "claude-sonnet-4-6"


def test_build_text_prompt_appends_custom_prompt():
    llm_cfg = {"custom_prompt": "always answer in Chinese"}
    prompt = build_text_prompt("base prompt", llm_cfg)
    assert "base prompt" in prompt
    assert "always answer in Chinese" in prompt
    assert "User Configured Global AI Instructions" in prompt


def test_build_text_prompt_keeps_original_when_custom_prompt_blank():
    assert build_text_prompt("base prompt", {"custom_prompt": "   "}) == "base prompt"


def test_build_system_prompt_appends_custom_prompt():
    llm_cfg = {"custom_prompt": "prefer concise output"}
    prompt = build_system_prompt("system prompt", llm_cfg)
    assert "system prompt" in prompt
    assert "prefer concise output" in prompt


def test_build_text_prompt_uses_latest_runtime_custom_prompt_when_requested(monkeypatch):
    from llm import text_runner

    monkeypatch.setattr(text_runner, "get_config", lambda: {"llm": {"custom_prompt": ""}})
    assert text_runner.build_text_prompt(
        "base prompt",
        {"custom_prompt": "stale prompt"},
        use_runtime_config=True,
    ) == "base prompt"


def test_build_text_prompt_uses_shared_bundle_header_when_legacy_path_missing(monkeypatch):
    prompt = build_text_prompt("base prompt", {"custom_prompt": "always answer in Chinese"})
    expected_header = PromptLoader().load("runtime_agent.llm_global_prompt_header").strip()

    assert prompt == f"base prompt\n\n{expected_header}\nalways answer in Chinese"


def test_complete_via_claude_cli_passes_model_to_subprocess(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    def fake_run(cmd, **kwargs):
        captured["cmd"] = cmd
        captured["env"] = kwargs["env"]
        return types.SimpleNamespace(stdout=b"ok\n", returncode=0)

    async def run_case():
        return await text_runner._complete_via_claude_cli(
            "base prompt",
            {
                "model": "claude-sonnet-4-6",
                "api_key": "secret-token",
                "base_url": "https://e-flowcode.cc",
            },
            None,
        )

    monkeypatch.setattr(text_runner.subprocess, "run", fake_run)

    result = asyncio.run(run_case())

    assert result == "ok"
    assert "--model" in captured["cmd"]
    assert captured["cmd"][captured["cmd"].index("--model") + 1] == "claude-sonnet-4-6"
    assert captured["env"]["ANTHROPIC_AUTH_TOKEN"] == "secret-token"
    assert captured["env"]["ANTHROPIC_API_KEY"] == "secret-token"
    assert captured["env"]["ANTHROPIC_BASE_URL"] == "https://e-flowcode.cc"
