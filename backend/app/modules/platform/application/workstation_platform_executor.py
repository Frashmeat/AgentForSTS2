from __future__ import annotations

import asyncio
import logging
from functools import partial
from typing import Any

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest, StepExecutionResult
from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionPollResult,
)
from app.modules.platform.runner.execution_adapter import ExecutionAdapter
from app.modules.platform.runner.step_dispatcher import StepDispatcher
from app.modules.platform.runner.workflow_registry import PlatformWorkflowRegistry, PlatformWorkflowStep
from app.modules.platform.runner.workflow_runner import WorkflowRunner


logger = logging.getLogger(__name__)


class WorkstationPlatformExecutor:
    def __init__(self, *, registry: PlatformWorkflowRegistry, runner: WorkflowRunner) -> None:
        self._registry = registry
        self._runner = runner

    def execute(self, request: WorkstationExecutionDispatchRequest) -> WorkstationExecutionPollResult:
        try:
            steps = self._registry.resolve(request.job_type, request.item_type, request.input_payload)
            results = asyncio.run(
                self._runner.run(
                    steps=steps,
                    base_request=StepExecutionRequest(
                        workflow_version=request.workflow_version,
                        step_protocol_version=request.step_protocol_version,
                        step_type="workflow.dispatch",
                        step_id="workflow.dispatch",
                        job_id=request.job_id,
                        job_item_id=request.job_item_id,
                        result_schema_version=request.result_schema_version,
                        input_payload=request.input_payload,
                        execution_binding=request.execution_binding,
                    ),
                )
            )
            final_result = results[-1] if results else StepExecutionResult(
                step_id="workflow.dispatch",
                status="failed_system",
                error_summary="workflow produced no result",
            )
            return WorkstationExecutionPollResult(
                workstation_execution_id=f"ws-exec-{request.execution_id}",
                status=final_result.status,
                step_id=final_result.step_id,
                output_payload=final_result.output_payload,
                error_summary=final_result.error_summary,
                error_payload=final_result.error_payload,
            )
        except Exception as exc:
            logger.exception(
                "workstation platform execution failed job_id=%s job_item_id=%s job_type=%s item_type=%s error=%s",
                request.job_id,
                request.job_item_id,
                request.job_type,
                request.item_type,
                str(exc)[:300],
            )
            return WorkstationExecutionPollResult(
                workstation_execution_id=f"ws-exec-{request.execution_id}",
                status="failed_system",
                step_id="workflow.dispatch",
                error_summary=str(exc),
                error_payload={"reason_code": "workstation_execution_failed"},
            )


def build_default_workstation_platform_executor(container: Any) -> WorkstationPlatformExecutor:
    from app.modules.platform.runner.asset_generate_handler import execute_asset_generate_step
    from app.modules.platform.runner.batch_custom_code_handler import execute_batch_custom_code_step
    from app.modules.platform.runner.build_project_handler import execute_build_project_step
    from app.modules.platform.runner.code_generate_handler import execute_code_generate_step
    from app.modules.platform.runner.log_analysis_handler import execute_log_analysis_step
    from app.modules.platform.runner.single_asset_plan_handler import execute_single_asset_plan_step
    from app.modules.platform.runner.text_generate_handler import execute_text_generate_step

    adapter = ExecutionAdapter(
        image_handler=None,
        code_handler=execute_code_generate_step,
        asset_handler=execute_asset_generate_step,
        text_handler=execute_text_generate_step,
        batch_custom_code_handler=execute_batch_custom_code_step,
        single_asset_plan_handler=execute_single_asset_plan_step,
        log_handler=execute_log_analysis_step,
        build_handler=partial(
            execute_build_project_step,
            deploy_target_lock_service=_build_server_deploy_target_lock_service(container),
        ),
        approval_handler=None,
    )
    return WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=WorkflowRunner(dispatcher=StepDispatcher(execute_handler=adapter.execute)),
    )


def build_workstation_workflow_registry() -> PlatformWorkflowRegistry:
    registry = PlatformWorkflowRegistry()

    def resolve_single_card_fullscreen(input_payload: dict[str, object]) -> list[PlatformWorkflowStep]:
        uploaded_asset_ref = str(input_payload.get("uploaded_asset_ref", "")).strip()
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if uploaded_asset_ref and server_project_ref:
            return [
                PlatformWorkflowStep("asset.generate", "single.card_fullscreen.asset", {"asset_type": "card_fullscreen"}),
                PlatformWorkflowStep("build.project", "single.card_fullscreen.build"),
            ]
        return [PlatformWorkflowStep("single.asset.plan", "single.card_fullscreen.plan", {"asset_type": "card_fullscreen"})]

    def resolve_batch_card_fullscreen(input_payload: dict[str, object]) -> list[PlatformWorkflowStep]:
        uploaded_asset_ref = str(input_payload.get("uploaded_asset_ref", "")).strip()
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if uploaded_asset_ref and server_project_ref:
            return [
                PlatformWorkflowStep("asset.generate", "batch.card_fullscreen.asset", {"asset_type": "card_fullscreen"}),
                PlatformWorkflowStep("build.project", "batch.card_fullscreen.build"),
            ]
        return [PlatformWorkflowStep("single.asset.plan", "batch.card_fullscreen.plan", {"asset_type": "card_fullscreen"})]

    registry.register("log_analysis", "log_analysis", [PlatformWorkflowStep("log.analyze", "log.analyze")])
    registry.register(
        "batch_generate",
        "custom_code",
        [
            PlatformWorkflowStep("batch.custom_code.plan", "batch.custom_code.plan"),
            PlatformWorkflowStep("code.generate", "batch.custom_code.codegen"),
            PlatformWorkflowStep("build.project", "batch.custom_code.build"),
        ],
    )
    registry.register(
        "single_generate",
        "custom_code",
        [
            PlatformWorkflowStep("batch.custom_code.plan", "single.custom_code.plan"),
            PlatformWorkflowStep("code.generate", "single.custom_code.codegen"),
            PlatformWorkflowStep("build.project", "single.custom_code.build"),
        ],
    )
    for job_type in ("batch_generate", "single_generate"):
        prefix = "batch" if job_type == "batch_generate" else "single"
        for item_type in ("card", "relic", "power", "character"):
            registry.register(
                job_type,
                item_type,
                [PlatformWorkflowStep("single.asset.plan", f"{prefix}.{item_type}.plan", {"asset_type": item_type})],
            )
    registry.register("batch_generate", "card_fullscreen", resolve_batch_card_fullscreen)
    registry.register("single_generate", "card_fullscreen", resolve_single_card_fullscreen)
    return registry


def _build_server_deploy_target_lock_service(container: Any) -> Any:
    factory = container.resolve_singleton("platform.server_deploy_target_lock_service_factory")
    if callable(factory):
        return factory()
    return factory
