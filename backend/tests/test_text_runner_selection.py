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
from llm.text_runner import TextRunner, build_system_prompt, build_text_prompt, resolve_model, resolve_text_backend


def test_text_runner_uses_cli_backend_when_mode_is_agent_cli():
    llm_cfg = {"mode": "agent_cli", "agent_backend": "codex"}
    assert resolve_text_backend(llm_cfg) == "codex_cli"


def test_text_runner_uses_litellm_when_mode_is_claude_api():
    llm_cfg = {"mode": "claude_api", "model": "claude-sonnet-4-6"}
    assert resolve_text_backend(llm_cfg) == "litellm"


def test_resolve_model_falls_back_to_default_claude_model():
    llm_cfg = {"mode": "claude_api"}
    assert resolve_model(llm_cfg) == "claude-sonnet-4-6"


def test_litellm_backend_accepts_text_backend_cwd_argument(monkeypatch, tmp_path):
    from llm import text_runner

    captured: dict[str, object] = {}

    async def fake_acompletion(**kwargs):
        captured.update(kwargs)
        return types.SimpleNamespace(
            choices=[
                types.SimpleNamespace(
                    message=types.SimpleNamespace(content="ok"),
                )
            ]
        )

    monkeypatch.setattr(text_runner.litellm, "acompletion", fake_acompletion)
    monkeypatch.setattr(text_runner, "get_config", lambda: {"llm": {"custom_prompt": ""}})

    result = asyncio.run(
        TextRunner(registry=text_runner._build_default_registry()).complete(
            "base prompt",
            {"mode": "claude_api", "model": "gpt-5.4", "api_key": "sk-test"},
            tmp_path,
        )
    )

    assert result == "ok"
    assert captured["model"] == "gpt-5.4"
    assert captured["api_key"] == "sk-test"


def test_litellm_openai_compatible_base_url_prefixes_bare_model(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    async def fake_acompletion(**kwargs):
        captured.update(kwargs)
        return types.SimpleNamespace(
            choices=[
                types.SimpleNamespace(
                    message=types.SimpleNamespace(content="ok"),
                )
            ]
        )

    monkeypatch.setattr(text_runner.litellm, "acompletion", fake_acompletion)

    result = asyncio.run(
        text_runner._complete_via_litellm(
            "base prompt",
            {
                "provider": "openai",
                "mode": "claude_api",
                "model": "deepseek-v3.2",
                "api_key": "sk-test",
                "base_url": "https://e-flowcode.cc/v1",
            },
            None,
        )
    )

    assert result == "ok"
    assert captured["model"] == "openai/deepseek-v3.2"
    assert captured["api_base"] == "https://e-flowcode.cc/v1"
    assert captured["extra_headers"]["User-Agent"] == "AgentTheSpire/0.1.0"


def test_litellm_openai_compatible_base_url_keeps_prefixed_model(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    async def fake_acompletion(**kwargs):
        captured.update(kwargs)
        return types.SimpleNamespace(
            choices=[
                types.SimpleNamespace(
                    message=types.SimpleNamespace(content="ok"),
                )
            ]
        )

    monkeypatch.setattr(text_runner.litellm, "acompletion", fake_acompletion)

    result = asyncio.run(
        text_runner._complete_via_litellm(
            "base prompt",
            {
                "provider": "openai",
                "mode": "claude_api",
                "model": "openai/deepseek-v3.2",
                "api_key": "sk-test",
                "base_url": "https://e-flowcode.cc/v1",
            },
            None,
        )
    )

    assert result == "ok"
    assert captured["model"] == "openai/deepseek-v3.2"
    assert captured["extra_headers"]["User-Agent"] == "AgentTheSpire/0.1.0"


def test_litellm_stream_passes_user_agent_header(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    class FakeStream:
        def __aiter__(self):
            return self

        async def __anext__(self):
            raise StopAsyncIteration

    async def fake_acompletion(**kwargs):
        captured.update(kwargs)
        return FakeStream()

    async def collect_chunk(_chunk: str) -> None:
        raise AssertionError("no chunks expected")

    monkeypatch.setattr(text_runner.litellm, "acompletion", fake_acompletion)

    result = asyncio.run(
        text_runner._stream_via_litellm(
            "system prompt",
            "user prompt",
            {
                "provider": "openai",
                "mode": "claude_api",
                "model": "deepseek-v3.2",
                "api_key": "sk-test",
                "base_url": "https://e-flowcode.cc/v1",
            },
            collect_chunk,
            None,
        )
    )

    assert result == ""
    assert captured["model"] == "openai/deepseek-v3.2"
    assert captured["stream"] is True
    assert captured["extra_headers"]["User-Agent"] == "AgentTheSpire/0.1.0"


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
    assert (
        text_runner.build_text_prompt(
            "base prompt",
            {"custom_prompt": "stale prompt"},
            use_runtime_config=True,
        )
        == "base prompt"
    )


def test_build_text_prompt_uses_shared_bundle_header_when_bundle_path_missing(monkeypatch):
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


def test_complete_via_claude_cli_uses_resolved_launcher(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    def fake_run(cmd, **kwargs):
        captured["cmd"] = cmd
        return types.SimpleNamespace(stdout=b"ok\n", returncode=0)

    async def run_case():
        return await text_runner._complete_via_claude_cli(
            "base prompt",
            {},
            None,
        )

    monkeypatch.setattr(
        text_runner,
        "_resolve_claude_launcher",
        lambda: ["C:/Program Files/nodejs/node.exe", "C:/Users/test/claude-code/cli.js"],
    )
    monkeypatch.setattr(text_runner.subprocess, "run", fake_run)

    result = asyncio.run(run_case())

    assert result == "ok"
    assert captured["cmd"][:2] == [
        "C:/Program Files/nodejs/node.exe",
        "C:/Users/test/claude-code/cli.js",
    ]
    assert "--print" in captured["cmd"]
    assert "-p" in captured["cmd"]


def test_complete_via_codex_cli_passes_execution_credentials_to_subprocess(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    def fake_run(cmd, **kwargs):
        captured["cmd"] = cmd
        captured["env"] = kwargs["env"]
        return types.SimpleNamespace(stdout=b"ok\n", stderr=b"", returncode=0)

    async def run_case():
        return await text_runner._complete_via_codex_cli(
            "base prompt",
            {
                "model": "gpt-5.4",
                "api_key": "sk-live-openai",
                "base_url": "https://api.openai.com/v1",
            },
            None,
        )

    monkeypatch.setattr(text_runner.shutil, "which", lambda _name: "C:/Tools/codex.cmd")
    monkeypatch.setattr(text_runner.subprocess, "run", fake_run)

    result = asyncio.run(run_case())

    assert result == "ok"
    assert captured["cmd"][0] == "C:/Tools/codex.cmd"
    assert captured["env"]["OPENAI_API_KEY"] == "sk-live-openai"
    assert captured["env"]["OPENAI_BASE_URL"] == "https://api.openai.com/v1"
