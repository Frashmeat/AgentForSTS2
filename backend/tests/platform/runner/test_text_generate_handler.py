from __future__ import annotations

import asyncio
import logging
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


def test_build_text_llm_config_uses_execution_binding_values():
    llm_cfg = build_text_llm_config(
        StepExecutionBinding(
            agent_backend="codex",
            provider="openai",
            model="gpt-5.4",
            credential="sk-live-openai",
            base_url="https://api.openai.com/v1",
        )
    )

    assert llm_cfg["mode"] == "claude_api"
    assert llm_cfg["agent_backend"] == "codex"
    assert llm_cfg["model"] == "gpt-5.4"
    assert llm_cfg["api_key"] == "sk-live-openai"
    assert llm_cfg["base_url"] == "https://api.openai.com/v1"


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
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                    base_url="https://api.openai.com/v1",
                ),
            ),
            complete_text_fn=fake_complete_text,
        )
    )

    assert result == {
        "text": "analysis result",
        "provider": "openai",
        "model": "gpt-5.4",
    }
    assert captured["prompt"] == "请分析这段日志"
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
                        agent_backend="codex",
                        provider="openai",
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
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                complete_text_fn=blocked_complete_text,
            )
        )
    except UpstreamTextGenerationBlockedError as error:
        payload = error.to_error_payload()
        assert payload["reason_code"] == "upstream_gateway_blocked"
        assert payload["upstream_category"] == "gateway_blocked"
        assert payload["retryable"] is False
        assert "上游网关拒绝" in str(error)
        assert "Your request was blocked" in payload["raw_error"]
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
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                complete_text_fn=blocked_complete_text,
            )
        )
    except UpstreamTextGenerationError as error:
        payload = error.to_error_payload()
        assert payload["reason_code"] == "upstream_content_policy_blocked"
        assert payload["upstream_category"] == "content_policy"
        assert payload["provider_error_code"] == "content_filter"
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
                            agent_backend="codex",
                            provider="openai",
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
