"""Tests for text runner backend resolution."""

import asyncio
import json
import subprocess
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
from llm.text_runner import (
    TextRunner,
    build_system_prompt,
    build_text_prompt,
    resolve_model,
    resolve_text_backend,
)


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


def test_litellm_openai_compatible_model_dump_error_falls_back_to_direct_http(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    async def broken_acompletion(**_kwargs):
        raise RuntimeError("litellm.APIError: OpenAIException - 'str' object has no attribute 'model_dump'")

    class FakeResponse:
        def raise_for_status(self) -> None:
            captured["raise_for_status"] = True

        def json(self) -> dict[str, object]:
            return {"choices": [{"message": {"content": " direct ok "}}]}

    class FakeAsyncClient:
        def __init__(self, **kwargs):
            captured["client_kwargs"] = kwargs

        async def __aenter__(self):
            return self

        async def __aexit__(self, exc_type, exc, tb):
            return None

        async def post(self, url, **kwargs):
            captured["url"] = url
            captured["post_kwargs"] = kwargs
            return FakeResponse()

    monkeypatch.setattr(text_runner.litellm, "acompletion", broken_acompletion)
    monkeypatch.setattr(text_runner.httpx, "AsyncClient", FakeAsyncClient)

    result = asyncio.run(
        text_runner._complete_via_litellm(
            "base prompt",
            {
                "provider": "openai",
                "mode": "claude_api",
                "model": "openai/deepseek-v4-pro",
                "api_key": "sk-test",
                "base_url": "https://e-flowcode.cc",
            },
            None,
        )
    )

    assert result == "direct ok"
    assert captured["url"] == "https://e-flowcode.cc/chat/completions"
    assert captured["raise_for_status"] is True
    post_kwargs = captured["post_kwargs"]
    assert post_kwargs["json"]["model"] == "deepseek-v4-pro"
    assert post_kwargs["json"]["messages"] == [{"role": "user", "content": "base prompt"}]
    assert post_kwargs["headers"]["Authorization"] == "Bearer sk-test"
    assert post_kwargs["headers"]["User-Agent"] == "AgentTheSpire/0.1.0"


def test_litellm_openai_compatible_model_dump_fallback_uses_error_protocol_not_provider(monkeypatch):
    from llm import text_runner

    captured: dict[str, object] = {}

    async def broken_acompletion(**_kwargs):
        raise RuntimeError("litellm.APIError: APIError: OpenAIException - 'str' object has no attribute 'model_dump'")

    class FakeResponse:
        def raise_for_status(self) -> None:
            pass

        def json(self) -> dict[str, object]:
            return {"choices": [{"message": {"content": "third party ok"}}]}

    class FakeAsyncClient:
        def __init__(self, **_kwargs):
            pass

        async def __aenter__(self):
            return self

        async def __aexit__(self, exc_type, exc, tb):
            return None

        async def post(self, url, **kwargs):
            captured["url"] = url
            captured["post_kwargs"] = kwargs
            return FakeResponse()

    monkeypatch.setattr(text_runner.litellm, "acompletion", broken_acompletion)
    monkeypatch.setattr(text_runner.httpx, "AsyncClient", FakeAsyncClient)

    result = asyncio.run(
        text_runner._complete_via_litellm(
            "base prompt",
            {
                "provider": "third-party",
                "mode": "claude_api",
                "model": "deepseek-v4-pro",
                "api_key": "sk-test",
                "base_url": "https://third-party.example/v1",
            },
            None,
        )
    )

    assert result == "third party ok"
    assert captured["url"] == "https://third-party.example/v1/chat/completions"
    assert captured["post_kwargs"]["json"]["model"] == "deepseek-v4-pro"


def test_litellm_openai_compatible_direct_fallback_reports_non_json_response(monkeypatch):
    from llm import text_runner

    async def broken_acompletion(**_kwargs):
        raise RuntimeError("litellm.APIError: APIError: OpenAIException - 'str' object has no attribute 'model_dump'")

    class FakeResponse:
        status_code = 200
        headers = {"content-type": "text/plain"}
        content = b"upstream gateway returned empty page"

        def raise_for_status(self) -> None:
            pass

        def json(self) -> dict[str, object]:
            raise json.JSONDecodeError("Expecting value", "", 0)

    class FakeAsyncClient:
        def __init__(self, **_kwargs):
            pass

        async def __aenter__(self):
            return self

        async def __aexit__(self, exc_type, exc, tb):
            return None

        async def post(self, _url, **_kwargs):
            return FakeResponse()

    monkeypatch.setattr(text_runner.litellm, "acompletion", broken_acompletion)
    monkeypatch.setattr(text_runner.httpx, "AsyncClient", FakeAsyncClient)

    try:
        asyncio.run(
            text_runner._complete_via_litellm(
                "base prompt",
                {
                    "provider": "openai",
                    "mode": "claude_api",
                    "model": "openai/deepseek-v4-pro",
                    "api_key": "sk-test",
                    "base_url": "https://e-flowcode.cc",
                },
                None,
            )
        )
    except RuntimeError as error:
        message = str(error)
        assert hasattr(error, "upstream_attempts")
        assert "OpenAI-compatible direct response was not valid JSON" in message
        assert "model_dump" in message
        assert "HTTP status 200" in message
        assert "content_type=text/plain" in message
        assert "body_tail=upstream gateway returned empty page" in message
        assert error.upstream_attempts[0]["stage"] == "litellm"
        assert error.upstream_attempts[1]["stage"] == "openai_compatible_direct"
        assert error.upstream_attempts[1]["endpoint"] == "https://e-flowcode.cc/chat/completions"
        assert "sk-test" not in message
        assert "base prompt" not in message
    else:
        raise AssertionError("expected invalid JSON response to be reported")


def test_litellm_anthropic_model_dump_error_without_openai_protocol_does_not_fallback(monkeypatch):
    from llm import text_runner

    async def broken_acompletion(**_kwargs):
        raise RuntimeError("AnthropicError: 'str' object has no attribute 'model_dump'")

    class UnexpectedAsyncClient:
        def __init__(self, **_kwargs):
            raise AssertionError("native Anthropic errors must not call OpenAI-compatible fallback")

    monkeypatch.setattr(text_runner.litellm, "acompletion", broken_acompletion)
    monkeypatch.setattr(text_runner.httpx, "AsyncClient", UnexpectedAsyncClient)

    try:
        asyncio.run(
            text_runner._complete_via_litellm(
                "base prompt",
                {
                    "provider": "anthropic",
                    "mode": "claude_api",
                    "model": "claude-sonnet-4-6",
                    "api_key": "sk-test",
                    "base_url": "https://api.anthropic.com",
                },
                None,
            )
        )
    except RuntimeError as error:
        assert "AnthropicError" in str(error)
    else:
        raise AssertionError("expected AnthropicError to be raised without direct fallback")


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
    monkeypatch.setenv("OPENAI_BASE_URL", "https://stale.example.invalid/v1")

    result = asyncio.run(run_case())

    assert result == "ok"
    assert captured["cmd"][0] == "C:/Tools/codex.cmd"
    assert captured["env"]["OPENAI_API_KEY"] == "sk-live-openai"
    assert "OPENAI_BASE_URL" not in captured["env"]
    assert "--ignore-user-config" in captured["cmd"]
    assert "-c" in captured["cmd"]
    config_values = [
        captured["cmd"][index + 1]
        for index, value in enumerate(captured["cmd"])
        if value == "-c"
    ]
    assert "model_provider=\"platform_openai_compatible\"" in config_values
    assert "model_providers.platform_openai_compatible.name=\"platform_openai_compatible\"" in config_values
    assert "model_providers.platform_openai_compatible.wire_api=\"responses\"" in config_values
    assert "model_providers.platform_openai_compatible.env_key=\"OPENAI_API_KEY\"" in config_values
    assert "model_providers.platform_openai_compatible.requires_openai_auth=false" in config_values
    assert "model_providers.platform_openai_compatible.supports_websockets=false" in config_values
    assert "model_providers.platform_openai_compatible.base_url=\"https://api.openai.com/v1\"" in config_values
    assert "sk-live-openai" not in " ".join(captured["cmd"])


def test_complete_via_codex_cli_logs_start_and_finish_without_secret(monkeypatch, caplog):
    from llm import text_runner

    def fake_run(cmd, **kwargs):
        return types.SimpleNamespace(stdout=b"ok\n", stderr=b"diagnostic\n", returncode=0)

    async def run_case():
        return await text_runner._complete_via_codex_cli(
            "base prompt",
            {
                "model": "gpt-5.2",
                "api_key": "sk-secret-should-not-log",
                "base_url": "https://api.openai.com/v1",
            },
            None,
        )

    monkeypatch.setattr(text_runner.shutil, "which", lambda _name: "/usr/local/bin/codex")
    monkeypatch.setattr(text_runner.subprocess, "run", fake_run)

    with caplog.at_level("INFO", logger="llm.text_runner"):
        result = asyncio.run(run_case())

    log_text = "\n".join(record.getMessage() for record in caplog.records)
    assert result == "ok"
    assert "text cli start backend=codex_cli" in log_text
    assert "text cli finished backend=codex_cli" in log_text
    assert "prompt_len=11" in log_text
    assert "timeout_seconds=180" in log_text
    assert "base_url=configured" in log_text
    assert "sk-secret-should-not-log" not in log_text


def test_complete_via_codex_cli_logs_timeout_diagnostics_without_secret(monkeypatch, caplog):
    from llm import text_runner

    def fake_run(cmd, **kwargs):
        raise subprocess.TimeoutExpired(cmd=cmd, timeout=180, output=b"partial stdout", stderr=b"partial stderr")

    async def run_case():
        return await text_runner._complete_via_codex_cli(
            "base prompt",
            {
                "model": "gpt-5.2",
                "api_key": "sk-timeout-secret",
                "base_url": "https://api.openai.com/v1",
            },
            None,
        )

    monkeypatch.setattr(text_runner.shutil, "which", lambda _name: "/usr/local/bin/codex")
    monkeypatch.setattr(text_runner.subprocess, "run", fake_run)

    with caplog.at_level("WARNING", logger="llm.text_runner"):
        try:
            asyncio.run(run_case())
        except subprocess.TimeoutExpired:
            pass
        else:
            raise AssertionError("expected TimeoutExpired")

    log_text = "\n".join(record.getMessage() for record in caplog.records)
    assert "text cli timeout backend=codex_cli" in log_text
    assert "stdout_tail=partial stdout" in log_text
    assert "stderr_tail=partial stderr" in log_text
    assert "timeout_seconds=180" in log_text
    assert "sk-timeout-secret" not in log_text
