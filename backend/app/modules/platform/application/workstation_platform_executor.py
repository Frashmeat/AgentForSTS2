from __future__ import annotations

import asyncio
import logging
import zipfile
from collections.abc import Callable
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest, StepExecutionResult
from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionEvent,
    WorkstationExecutionPollResult,
)
from app.modules.platform.runner.execution_adapter import ExecutionAdapter
from app.modules.platform.runner.step_dispatcher import StepDispatcher
from app.modules.platform.runner.workflow_registry import PlatformWorkflowRegistry, PlatformWorkflowStep
from app.modules.platform.runner.workflow_runner import WorkflowRunner

logger = logging.getLogger(__name__)
_SOURCE_PACKAGE_SKIP_DIRS = {"bin", "obj", ".godot", ".git"}
WorkstationEventSink = Callable[[WorkstationExecutionEvent], None]


class WorkstationPlatformExecutor:
    def __init__(self, *, registry: PlatformWorkflowRegistry, runner: WorkflowRunner) -> None:
        self._registry = registry
        self._runner = runner

    def execute(
        self,
        request: WorkstationExecutionDispatchRequest,
        event_sink: WorkstationEventSink | None = None,
    ) -> WorkstationExecutionPollResult:
        events: list[WorkstationExecutionEvent] = []
        event_sequence = 0

        def publish_event(event_type: str, step_id: str) -> None:
            nonlocal event_sequence
            event_sequence += 1
            event = WorkstationExecutionEvent(
                sequence=event_sequence,
                event_type=f"workstation.{event_type}",
                occurred_at=datetime.now(UTC).isoformat(),
                payload=_step_event_payload(
                    event_type=event_type,
                    step_id=step_id,
                    step_type=step_types_by_id.get(step_id, ""),
                ),
            )
            events.append(event)
            if event_sink is not None:
                event_sink(event)

        try:
            steps = self._registry.resolve(request.job_type, request.item_type, request.input_payload)
            step_types_by_id = {step.step_id: step.step_type for step in steps}
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
                    event_publisher=publish_event,
                )
            )
            final_result = (
                results[-1]
                if results
                else StepExecutionResult(
                    step_id="workflow.dispatch",
                    status="failed_system",
                    error_summary="workflow produced no result",
                )
            )
            output_payload = dict(final_result.output_payload)
            if final_result.status == "succeeded":
                self._append_source_project_artifact(output_payload)
            return WorkstationExecutionPollResult(
                workstation_execution_id=f"ws-exec-{request.execution_id}",
                status=final_result.status,
                step_id=final_result.step_id,
                output_payload=output_payload,
                error_summary=final_result.error_summary,
                error_payload=final_result.error_payload,
                events=events,
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
                events=events,
            )

    def _append_source_project_artifact(self, output_payload: dict[str, object]) -> None:
        root_text = str(output_payload.get("server_workspace_root", "")).strip()
        if not root_text:
            return
        project_root = Path(root_text)
        if not project_root.exists() or not project_root.is_dir():
            return
        package_path = _create_source_project_package(project_root)
        artifacts = output_payload.get("artifacts")
        if not isinstance(artifacts, list):
            artifacts = []
        artifacts.append(
            {
                "artifact_type": "source_project",
                "storage_provider": "server_workspace",
                "object_key": str(package_path),
                "file_name": package_path.name,
                "mime_type": "application/zip",
                "size_bytes": package_path.stat().st_size,
                "result_summary": "服务器生成项目包",
            }
        )
        output_payload["artifacts"] = artifacts


def build_default_workstation_platform_executor(container: Any) -> WorkstationPlatformExecutor:
    from app.modules.platform.runner.asset_generate_handler import execute_asset_generate_step
    from app.modules.platform.runner.batch_custom_code_handler import execute_batch_custom_code_step
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
        build_handler=None,
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
                PlatformWorkflowStep(
                    "asset.generate", "single.card_fullscreen.asset", {"asset_type": "card_fullscreen"}
                ),
            ]
        return [
            PlatformWorkflowStep("single.asset.plan", "single.card_fullscreen.plan", {"asset_type": "card_fullscreen"})
        ]

    def resolve_batch_card_fullscreen(input_payload: dict[str, object]) -> list[PlatformWorkflowStep]:
        uploaded_asset_ref = str(input_payload.get("uploaded_asset_ref", "")).strip()
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if uploaded_asset_ref and server_project_ref:
            return [
                PlatformWorkflowStep(
                    "asset.generate", "batch.card_fullscreen.asset", {"asset_type": "card_fullscreen"}
                ),
            ]
        return [
            PlatformWorkflowStep("single.asset.plan", "batch.card_fullscreen.plan", {"asset_type": "card_fullscreen"})
        ]

    registry.register("log_analysis", "log_analysis", [PlatformWorkflowStep("log.analyze", "log.analyze")])
    registry.register(
        "batch_generate",
        "custom_code",
        [
            PlatformWorkflowStep("batch.custom_code.plan", "batch.custom_code.plan"),
            PlatformWorkflowStep("code.generate", "batch.custom_code.codegen"),
        ],
    )
    registry.register(
        "single_generate",
        "custom_code",
        [
            PlatformWorkflowStep("batch.custom_code.plan", "single.custom_code.plan"),
            PlatformWorkflowStep("code.generate", "single.custom_code.codegen"),
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


def _create_source_project_package(project_root: Path) -> Path:
    artifact_dir = project_root.parent / "_source_artifacts"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    package_path = artifact_dir / f"{project_root.name}.source.zip"
    if package_path.exists():
        package_path.unlink()
    with zipfile.ZipFile(package_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(project_root.rglob("*")):
            if path.is_dir():
                continue
            relative = path.relative_to(project_root)
            if any(part in _SOURCE_PACKAGE_SKIP_DIRS for part in relative.parts):
                continue
            archive.write(path, arcname=str(relative).replace("\\", "/"))
    return package_path


def _step_event_payload(*, event_type: str, step_id: str, step_type: str) -> dict[str, object]:
    return {
        "phase": _phase_for_step_type(step_type),
        "step_id": step_id,
        "step_type": step_type,
        "message": _message_for_step_event(event_type=event_type, step_type=step_type),
    }


def _phase_for_step_type(step_type: str) -> str:
    if step_type in {"batch.custom_code.plan", "single.asset.plan", "log.analyze"}:
        return "planning"
    if step_type == "code.generate":
        return "code_generation"
    if step_type == "asset.generate":
        return "asset_generation"
    return "execution"


def _message_for_step_event(*, event_type: str, step_type: str) -> str:
    if event_type == "step.finished":
        return "已完成当前步骤"
    if step_type == "code.generate":
        return "正在生成代码"
    if step_type == "asset.generate":
        return "正在生成资产"
    if step_type == "log.analyze":
        return "正在分析日志"
    return "正在生成方案"
