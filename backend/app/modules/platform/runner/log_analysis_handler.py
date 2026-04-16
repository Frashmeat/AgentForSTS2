from __future__ import annotations

import os
import re
from collections.abc import Awaitable, Callable
from pathlib import Path

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.shared.prompting import PromptLoader

from .text_generate_handler import execute_text_generate_step


TextStepExecutor = Callable[[StepExecutionRequest], Awaitable[dict[str, object]]]

_LOG_PATH = Path(os.environ.get("APPDATA", "")) / "SlayTheSpire2" / "logs" / "godot.log"
_MAX_LINES = 300
_PROMPT_LOADER = PromptLoader()
_LOG_ANALYZER_USER_PROMPT_KEY = "analyzer.log_analyzer_user"
_LOG_ANALYZER_EXTRA_CONTEXT_PROMPT_KEY = "analyzer.log_analyzer_extra_context"


def _read_log() -> tuple[str, bool]:
    if not _LOG_PATH.exists():
        return "", False

    text = _LOG_PATH.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()
    tail = lines[-_MAX_LINES:]
    error_pattern = re.compile(r"error|exception|critical|crash|fail", re.IGNORECASE)
    extra = [line for line in lines[:-_MAX_LINES] if error_pattern.search(line)]

    combined: list[str] = []
    if extra:
        combined.append(_PROMPT_LOADER.load("analyzer.log_excerpt_header").strip())
        combined.extend(extra[-100:])
        combined.append(_PROMPT_LOADER.load("analyzer.log_tail_header").strip())
    combined.extend(tail)
    return "\n".join(combined), True


def _build_prompt(extra_context: str) -> str:
    log_content, exists = _read_log()
    if not exists:
        return _PROMPT_LOADER.render("analyzer.log_missing_message", {"log_path": _LOG_PATH}).strip()

    extra_context_block = ""
    if extra_context:
        extra_context_block = _PROMPT_LOADER.render(
            _LOG_ANALYZER_EXTRA_CONTEXT_PROMPT_KEY,
            {"extra_context": extra_context},
        )

    return _PROMPT_LOADER.render(
        _LOG_ANALYZER_USER_PROMPT_KEY,
        {
            "extra_context_block": extra_context_block,
            "log_content": log_content,
            "log_path": _LOG_PATH,
        },
    )


async def execute_log_analysis_step(
    request: StepExecutionRequest,
    *,
    text_step_executor: TextStepExecutor | None = None,
) -> dict[str, object]:
    log_content, exists = _read_log()
    if not exists:
        raise FileNotFoundError(_PROMPT_LOADER.render("analyzer.log_missing_message", {"log_path": _LOG_PATH}).strip())

    if text_step_executor is None:
        text_step_executor = execute_text_generate_step

    prompt = _build_prompt(str(request.input_payload.get("context", "")).strip())
    forwarded_request = StepExecutionRequest(
        workflow_version=request.workflow_version,
        step_protocol_version=request.step_protocol_version,
        step_type="text.generate",
        step_id=f"{request.step_id}.text",
        job_id=request.job_id,
        job_item_id=request.job_item_id,
        result_schema_version=request.result_schema_version,
        input_payload={"prompt": prompt},
        execution_binding=request.execution_binding,
    )
    result = await text_step_executor(forwarded_request)
    payload = dict(result)
    payload["log_lines"] = len(log_content.splitlines())
    return payload
