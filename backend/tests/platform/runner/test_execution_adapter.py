from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.modules.platform.runner.execution_adapter import ExecutionAdapter
from app.modules.platform.runner.step_dispatcher import StepDispatcher


def test_execution_adapter_dispatches_to_registered_step_handlers():
    calls: list[str] = []

    async def image_handler(request: StepExecutionRequest):
        calls.append(f"image:{request.step_id}")
        return {"artifact_type": "image"}

    async def code_handler(request: StepExecutionRequest):
        calls.append(f"code:{request.step_id}")
        return {"artifact_type": "code"}

    async def text_handler(request: StepExecutionRequest):
        calls.append(f"text:{request.step_id}")
        return {"artifact_type": "text"}

    adapter = ExecutionAdapter(
        image_handler=image_handler,
        code_handler=code_handler,
        text_handler=text_handler,
        build_handler=None,
        approval_handler=None,
    )
    dispatcher = StepDispatcher(execute_handler=adapter.execute)

    image_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="image.generate",
                step_id="img-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )
    code_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="code.generate",
                step_id="code-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )
    text_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="text.generate",
                step_id="text-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )

    assert calls == ["image:img-1", "code:code-1", "text:text-1"]
    assert image_result.output_payload["artifact_type"] == "image"
    assert code_result.output_payload["artifact_type"] == "code"
    assert text_result.output_payload["artifact_type"] == "text"


def test_execution_adapter_classifies_handler_errors_as_failed_system():
    async def broken_handler(request: StepExecutionRequest):
        raise RuntimeError("network down")

    adapter = ExecutionAdapter(
        image_handler=broken_handler,
        code_handler=None,
        text_handler=None,
        build_handler=None,
        approval_handler=None,
    )

    result = asyncio.run(
        adapter.execute(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="image.generate",
                step_id="img-fail",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )

    assert result.status == "failed_system"
    assert "network down" in result.error_summary
