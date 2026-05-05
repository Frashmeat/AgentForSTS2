from __future__ import annotations

import asyncio
import logging
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.text_generate_handler import (
    UpstreamTextGenerationBlockedError,
    UpstreamTextGenerationError,
    build_text_llm_config,
    execute_text_generate_step,
)


def test_build_text_llm_config_uses_codex_cli_mode_for_codex_binding():
    llm_cfg = build_text_llm_config(
        StepExecutionBinding(
            runner_type="codex_cli",
            api_protocol="openai_compatible",
            model="gpt-5.4",
            credential="sk-live-openai",
            api_base_url="https://api.openai.com/v1",
        )
    )

    assert llm_cfg["mode"] == "agent_cli"
    assert llm_cfg["agent_backend"] == "codex"
    assert llm_cfg["provider"] == "openai"
    assert llm_cfg["model"] == "gpt-5.4"
    assert llm_cfg["api_key"] == "sk-live-openai"
    assert llm_cfg["base_url"] == "https://api.openai.com/v1"


def test_build_text_llm_config_rejects_cli_protocol_mismatch():
    try:
        build_text_llm_config(
            StepExecutionBinding(
                runner_type="claude_cli",
                api_protocol="openai_compatible",
                model="deepseek-v4-pro",
                credential="sk-live-openai",
                api_base_url="https://api.openai.com/v1",
            )
        )
    except ValueError as error:
        assert "api_protocol must be one of anthropic_compatible" in str(error)
    else:
        raise AssertionError("expected ValueError for claude_cli with openai_compatible")


def test_execute_text_generate_step_uses_execution_binding_to_call_text_runner():
    captured: dict[str, object] = {}

    async def fake_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        captured["prompt"] = prompt
        captured["llm_cfg"] = dict(llm_cfg)
        captured["cwd"] = cwd
        return "analysis result"

    result = asyncio.run(
        execute_text_generate_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="text.generate",
                step_id="text-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={"prompt": "请分析这段日志"},
                execution_binding=StepExecutionBinding(
                    runner_type="codex_cli",
                    api_protocol="openai_compatible",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                    api_base_url="https://api.openai.com/v1",
                ),
            ),
            complete_text_fn=fake_complete_text,
        )
    )

    assert result == {
        "text": "analysis result",
        "api_protocol": "openai_compatible",
        "model": "gpt-5.4",
    }
    assert captured["prompt"] == "请分析这段日志"
    assert captured["llm_cfg"]["provider"] == "openai"
    assert captured["llm_cfg"]["api_key"] == "sk-live-openai"
    assert captured["llm_cfg"]["model"] == "gpt-5.4"


def test_execute_text_generate_step_requires_prompt():
    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-2",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    execution_binding=StepExecutionBinding(
                        runner_type="codex_cli",
                        api_protocol="openai_compatible",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                )
            )
        )
    except ValueError as error:
        assert str(error) == "input_payload.prompt is required"
    else:
        raise AssertionError("expected ValueError when prompt is missing")


def test_execute_text_generate_step_classifies_generic_request_blocked_as_gateway(caplog):
    async def blocked_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise RuntimeError("litellm.APIError: APIError: OpenAIException - Your request was blocked.")

    caplog.set_level(logging.WARNING)
    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-3",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"prompt": "虚构游戏机制：造成伤害。"},
                    execution_binding=StepExecutionBinding(
                        runner_type="codex_cli",
                        api_protocol="openai_compatible",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                complete_text_fn=blocked_complete_text,
            )
        )
    except UpstreamTextGenerationBlockedError as error:
        payload = error.to_error_payload()
        assert payload["schema_version"] == "platform_error.v1"
        assert payload["origin"] == "upstream"
        assert payload["runtime_surface"] == "web"
        assert payload["reason_code"] == "upstream_gateway_blocked"
        assert payload["category"] == "gateway_blocked"
        assert payload["retryable"] is False
        assert "上游网关拒绝" in str(error)
        assert "Your request was blocked" in payload["developer_message"]
        assert any(
            record.levelno == logging.WARNING
            and "platform text generation upstream failed" in record.message
            and "reason_code=upstream_gateway_blocked" in record.message
            and "job_id=1" in record.message
            and "job_item_id=2" in record.message
            for record in caplog.records
        )
    else:
        raise AssertionError("expected UpstreamTextGenerationBlockedError")


def test_execute_text_generate_step_classifies_content_filter():
    async def blocked_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise RuntimeError("OpenAIException: content_filter policy triggered")

    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-4",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"prompt": "虚构游戏机制：造成伤害。"},
                    execution_binding=StepExecutionBinding(
                        runner_type="codex_cli",
                        api_protocol="openai_compatible",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                complete_text_fn=blocked_complete_text,
            )
        )
    except UpstreamTextGenerationError as error:
        payload = error.to_error_payload()
        assert payload["schema_version"] == "platform_error.v1"
        assert payload["origin"] == "upstream"
        assert payload["reason_code"] == "upstream_content_policy_blocked"
        assert payload["category"] == "content_policy"
        assert payload["provider_error_code"] == "content_filter"
    else:
        raise AssertionError("expected UpstreamTextGenerationError")


def test_execute_text_generate_step_classifies_invalid_openai_compatible_response():
    async def invalid_response_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise RuntimeError(
            "OpenAI-compatible direct response was not valid JSON HTTP status 200 "
            "content_type=text/plain body_tail=upstream gateway returned empty page"
        )

    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-invalid-json",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"prompt": "虚构游戏机制：造成伤害。"},
                    execution_binding=StepExecutionBinding(
                        runner_type="api",
                        api_protocol="openai_compatible",
                        model="deepseek-v4-pro",
                        credential="sk-live-openai",
                        api_base_url="https://e-flowcode.cc",
                    ),
                ),
                complete_text_fn=invalid_response_complete_text,
            )
        )
    except UpstreamTextGenerationError as error:
        payload = error.to_error_payload()
        assert payload["schema_version"] == "platform_error.v1"
        assert payload["origin"] == "upstream"
        assert payload["reason_code"] == "upstream_invalid_response"
        assert payload["category"] == "invalid_response"
        assert payload["retryable"] is True
        assert payload["http_status"] == 200
        assert payload["provider_error_code"] == "invalid_json_response"
    else:
        raise AssertionError("expected UpstreamTextGenerationError")


def test_execute_text_generate_step_preserves_upstream_attempt_chain_for_protocol_mismatch():
    class FallbackError(RuntimeError):
        def __init__(self) -> None:
            super().__init__(
                "OpenAI-compatible fallback failed after LiteLLM response parsing error; "
                "initial_error=litellm.APIError: 'str' object has no attribute 'model_dump'; "
                "final_error=OpenAI-compatible direct response was not valid JSON HTTP status 200 "
                "content_type=text/html; charset=utf-8 body_tail=<title>New API</title><meta name=\"title\""
            )
            self.initial_error = RuntimeError("litellm.APIError: 'str' object has no attribute 'model_dump'")
            self.final_error = RuntimeError(
                "OpenAI-compatible direct response was not valid JSON HTTP status 200 "
                "content_type=text/html; charset=utf-8 body_tail=<title>New API</title>"
            )
            self.endpoint = "https://e-flowcode.cc/chat/completions"
            self.upstream_attempts = [
                {"stage": "litellm", "error": str(self.initial_error), "exception_type": "RuntimeError"},
                {
                    "stage": "openai_compatible_direct",
                    "endpoint": self.endpoint,
                    "error": str(self.final_error),
                    "exception_type": "RuntimeError",
                },
            ]

    async def mismatch_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise FallbackError()

    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-protocol-mismatch",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"prompt": "虚构游戏机制：造成伤害。"},
                    execution_binding=StepExecutionBinding(
                        runner_type="api",
                        api_protocol="openai_compatible",
                        model="deepseek-v4-pro",
                        credential="sk-live-openai",
                        api_base_url="https://e-flowcode.cc",
                    ),
                ),
                complete_text_fn=mismatch_complete_text,
            )
        )
    except UpstreamTextGenerationError as error:
        payload = error.to_error_payload()
        diagnostic = payload["diagnostic"]
        assert payload["reason_code"] == "upstream_protocol_mismatch"
        assert payload["provider_error_code"] == "api_protocol_mismatch"
        assert payload["retryable"] is False
        assert diagnostic["endpoint"] == "https://e-flowcode.cc/chat/completions"
        assert "model_dump" in diagnostic["initial_error"]
        assert "content_type=text/html" in diagnostic["final_error"]
        assert diagnostic["upstream_attempts"][0]["stage"] == "litellm"
        assert diagnostic["upstream_attempts"][1]["stage"] == "openai_compatible_direct"
    else:
        raise AssertionError("expected UpstreamTextGenerationError")


def test_execute_text_generate_step_keeps_web_workstation_surface_for_upstream_errors():
    async def rate_limited_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise RuntimeError("HTTP status 429 rate limit exceeded")

    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-web-workstation",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={
                        "prompt": "虚构游戏机制：造成伤害。",
                        "__runtime_surface": "web_workstation",
                    },
                    execution_binding=StepExecutionBinding(
                        runner_type="api",
                        api_protocol="openai_compatible",
                        model="deepseek-v4-pro",
                        credential="sk-live-openai",
                        api_base_url="https://e-flowcode.cc",
                    ),
                ),
                complete_text_fn=rate_limited_complete_text,
            )
        )
    except UpstreamTextGenerationError as error:
        payload = error.to_error_payload()
        assert payload["origin"] == "upstream"
        assert payload["runtime_surface"] == "web_workstation"
        assert payload["reason_code"] == "upstream_rate_limited"
    else:
        raise AssertionError("expected UpstreamTextGenerationError")


def test_execute_text_generate_step_classifies_auth_and_rate_limit():
    async def auth_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise RuntimeError("HTTP status 403 permission denied for model")

    async def rate_limited_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise RuntimeError("HTTP status 429 rate limit exceeded")

    for complete_text, expected_code, expected_retryable in (
        (auth_complete_text, "upstream_auth_or_region_blocked", False),
        (rate_limited_complete_text, "upstream_rate_limited", True),
    ):
        try:
            asyncio.run(
                execute_text_generate_step(
                    StepExecutionRequest(
                        workflow_version="2026.03.31",
                        step_protocol_version="v1",
                        step_type="text.generate",
                        step_id="text-5",
                        job_id=1,
                        job_item_id=2,
                        result_schema_version="v1",
                        input_payload={"prompt": "虚构游戏机制：造成伤害。"},
                        execution_binding=StepExecutionBinding(
                            runner_type="codex_cli",
                            api_protocol="openai_compatible",
                            model="gpt-5.4",
                            credential="sk-live-openai",
                        ),
                    ),
                    complete_text_fn=complete_text,
                )
            )
        except UpstreamTextGenerationError as error:
            payload = error.to_error_payload()
            assert payload["reason_code"] == expected_code
            assert payload["retryable"] is expected_retryable
        else:
            raise AssertionError("expected UpstreamTextGenerationError")


def test_execute_text_generate_step_classifies_cli_timeout_with_diagnostics(caplog):
    async def timeout_complete_text(prompt: str, llm_cfg: dict, cwd=None) -> str:
        raise subprocess.TimeoutExpired(
            cmd=["/usr/local/bin/codex", "exec", "-"],
            timeout=180,
            output=b"partial stdout",
            stderr=b"partial stderr",
        )

    caplog.set_level(logging.WARNING)
    try:
        asyncio.run(
            execute_text_generate_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="text.generate",
                    step_id="text-timeout",
                    job_id=10,
                    job_item_id=20,
                    result_schema_version="v1",
                    input_payload={"prompt": "虚构游戏机制：造成伤害。"},
                    execution_binding=StepExecutionBinding(
                        runner_type="codex_cli",
                        api_protocol="openai_compatible",
                        model="gpt-5.2",
                        credential="sk-live-openai",
                    ),
                ),
                complete_text_fn=timeout_complete_text,
            )
        )
    except UpstreamTextGenerationBlockedError as error:
        payload = error.to_error_payload()
        assert payload["schema_version"] == "platform_error.v1"
        assert payload["origin"] == "upstream"
        assert payload["reason_code"] == "llm_cli_timeout"
        assert payload["category"] == "timeout"
        assert payload["provider_error_code"] == "timeout"
        assert payload["retryable"] is True
        assert "超过超时时间" in str(error)
        assert "partial stdout" in payload["developer_message"]
        assert "partial stderr" in payload["developer_message"]
        assert any(
            record.levelno == logging.WARNING
            and "platform text generation upstream failed" in record.message
            and "reason_code=llm_cli_timeout" in record.message
            and "job_id=10" in record.message
            and "job_item_id=20" in record.message
            for record in caplog.records
        )
    else:
        raise AssertionError("expected UpstreamTextGenerationBlockedError")
