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

    async def asset_handler(request: StepExecutionRequest):
        calls.append(f"asset:{request.step_id}")
        return {"artifact_type": "asset"}

    async def text_handler(request: StepExecutionRequest):
        calls.append(f"text:{request.step_id}")
        return {"artifact_type": "text"}

    async def batch_custom_code_handler(request: StepExecutionRequest):
        calls.append(f"batch-custom-code:{request.step_id}")
        return {"artifact_type": "batch-custom-code"}

    async def single_asset_plan_handler(request: StepExecutionRequest):
        calls.append(f"single-asset-plan:{request.step_id}")
        return {"artifact_type": "single-asset-plan"}

    async def log_handler(request: StepExecutionRequest):
        calls.append(f"log:{request.step_id}")
        return {"artifact_type": "log"}

    adapter = ExecutionAdapter(
        image_handler=image_handler,
        code_handler=code_handler,
        asset_handler=asset_handler,
        text_handler=text_handler,
        batch_custom_code_handler=batch_custom_code_handler,
        single_asset_plan_handler=single_asset_plan_handler,
        log_handler=log_handler,
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
    asset_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="asset.generate",
                step_id="asset-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )
    log_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="log.analyze",
                step_id="log-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )
    batch_custom_code_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="batch.custom_code.plan",
                step_id="batch-custom-code-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )
    single_asset_plan_result = asyncio.run(
        dispatcher.dispatch(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="single.asset.plan",
                step_id="single-asset-plan-1",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            )
        )
    )

    assert calls == [
        "image:img-1",
        "code:code-1",
        "text:text-1",
        "asset:asset-1",
        "log:log-1",
        "batch-custom-code:batch-custom-code-1",
        "single-asset-plan:single-asset-plan-1",
    ]
    assert image_result.output_payload["artifact_type"] == "image"
    assert code_result.output_payload["artifact_type"] == "code"
    assert text_result.output_payload["artifact_type"] == "text"
    assert asset_result.output_payload["artifact_type"] == "asset"
    assert log_result.output_payload["artifact_type"] == "log"
    assert batch_custom_code_result.output_payload["artifact_type"] == "batch-custom-code"
    assert single_asset_plan_result.output_payload["artifact_type"] == "single-asset-plan"


def test_execution_adapter_classifies_handler_errors_as_failed_system():
    async def broken_handler(request: StepExecutionRequest):
        raise RuntimeError("network down")

    adapter = ExecutionAdapter(
        image_handler=broken_handler,
        code_handler=None,
        asset_handler=None,
        text_handler=None,
        batch_custom_code_handler=None,
        single_asset_plan_handler=None,
        log_handler=None,
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
