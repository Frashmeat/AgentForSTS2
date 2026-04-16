from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.log_analysis_handler import execute_log_analysis_step


def test_execute_log_analysis_step_reads_log_and_delegates_to_text_generation(monkeypatch):
    captured: dict[str, object] = {}

    async def fake_text_step(request: StepExecutionRequest):
        captured["request"] = request
        return {
            "text": "分析完成",
            "provider": request.execution_binding.provider,
            "model": request.execution_binding.model,
        }

    monkeypatch.setattr(
        "app.modules.platform.runner.log_analysis_handler._read_log",
        lambda: ("line1\nline2", True),
    )
    monkeypatch.setattr(
        "app.modules.platform.runner.log_analysis_handler._build_prompt",
        lambda context: f"prompt:{context}",
    )

    result = asyncio.run(
        execute_log_analysis_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="log.analyze",
                step_id="log-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={"context": "黑屏了"},
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                ),
            ),
            text_step_executor=fake_text_step,
        )
    )

    forwarded = captured["request"]
    assert isinstance(forwarded, StepExecutionRequest)
    assert forwarded.step_type == "text.generate"
    assert forwarded.input_payload["prompt"] == "prompt:黑屏了"
    assert result["text"] == "分析完成"
    assert result["log_lines"] == 2


def test_execute_log_analysis_step_fails_when_log_missing(monkeypatch):
    monkeypatch.setattr(
        "app.modules.platform.runner.log_analysis_handler._read_log",
        lambda: ("", False),
    )

    try:
        asyncio.run(
            execute_log_analysis_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="log.analyze",
                    step_id="log-2",
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
    except FileNotFoundError as error:
        assert "游戏日志文件不存在" in str(error)
    else:
        raise AssertionError("expected FileNotFoundError when log file is missing")
