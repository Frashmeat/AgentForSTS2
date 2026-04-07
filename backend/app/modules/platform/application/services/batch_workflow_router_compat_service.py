from __future__ import annotations

import json
import traceback
from datetime import UTC, datetime
from importlib import import_module
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Awaitable, Callable

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.job_application_service import JobApplicationService
from app.modules.platform.contracts.job_commands import CreateJobCommand, CreateJobItemInput, StartJobCommand
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.repositories.ai_execution_repository_sqlalchemy import (
    AIExecutionRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_event_repository_sqlalchemy import (
    JobEventRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.modules.platform.runner.step_dispatcher import StepDispatcher
from app.modules.platform.runner.workflow_registry import PlatformWorkflowRegistry, PlatformWorkflowStep
from app.modules.platform.runner.workflow_runner import WorkflowRunner
from app.shared.infra.ws_errors import send_ws_error


CodegenFn = Callable[..., Awaitable[str]]


def _fallback_plan_from_payload(payload: dict[str, Any]) -> SimpleNamespace:
    return SimpleNamespace(
        mod_name=payload.get("mod_name", ""),
        summary=payload.get("summary", ""),
        items=[SimpleNamespace(**item) for item in payload.get("items", [])],
    )


def _parse_plan(payload: dict[str, Any]) -> SimpleNamespace:
    planner = import_module("agents.planner")
    candidate = planner.plan_from_dict(payload)
    items = getattr(candidate, "items", None)
    if hasattr(candidate, "summary") and isinstance(items, list) and (
        len(items) == len(payload.get("items", [])) or not payload.get("items")
    ):
        return candidate
    return _fallback_plan_from_payload(payload)


class BatchWorkflowRouterCompatService:
    def __init__(
        self,
        *,
        session_factory=None,
        create_custom_code_fn: CodegenFn | None = None,
    ) -> None:
        self.session_factory = session_factory
        self.create_custom_code_fn = create_custom_code_fn

    def plan(self, body: dict):
        from routers import batch_workflow as batch_router

        return batch_router._legacy_api_plan(body)

    async def handle_ws_batch(self, ws) -> None:
        if self.session_factory is None:
            from routers import batch_workflow as batch_router

            await batch_router._handle_legacy_ws_batch(ws)
            return

        await ws.accept()
        raw = await ws.receive_text()
        params = json.loads(raw)
        if params.get("action") == "start_with_plan":
            plan = _parse_plan(params["plan"])
            if all(item.type == "custom_code" and not item.needs_image for item in plan.items):
                await self._handle_platform_custom_code_plan(ws, params, plan)
                return

        from routers import batch_workflow as batch_router

        await batch_router._handle_legacy_ws_batch(ws, initial_params=params)

    async def _handle_platform_custom_code_plan(self, ws, params: dict, plan) -> None:
        project_root = Path(params["project_root"])
        user_id = int(params.get("user_id", 1001))
        create_custom_code = self.create_custom_code_fn
        if create_custom_code is None:
            from agents.code_agent import create_custom_code as default_create_custom_code

            create_custom_code = default_create_custom_code

        session = self.session_factory()
        try:
            job_repository = JobRepositorySqlAlchemy(session)
            event_repository = JobEventRepositorySqlAlchemy(session)
            application_service = JobApplicationService(job_repository, event_repository)

            job = application_service.create_job(
                user_id=user_id,
                command=CreateJobCommand(
                    job_type="batch_generate",
                    workflow_version="2026.03.31",
                    input_summary=plan.summary,
                    items=[
                        CreateJobItemInput(
                            item_type=item.type,
                            input_summary=item.description,
                            input_payload={
                                "asset_name": item.name,
                                "description": item.description,
                                "implementation_notes": item.implementation_notes,
                                "project_root": str(project_root),
                            },
                        )
                        for item in plan.items
                    ],
                ),
            )
            started = application_service.start_job(
                user_id=user_id,
                command=StartJobCommand(job_id=job.id, triggered_by="platform_runner"),
            )
            if started is None:
                raise RuntimeError("failed to start platform batch job")

            registry = PlatformWorkflowRegistry(
                {
                    ("batch_generate", "custom_code"): [
                        PlatformWorkflowStep(step_type="code.generate", step_id="code.generate")
                    ]
                }
            )

            async def execute_handler(request) -> StepExecutionResult:
                output = await create_custom_code(
                    description=str(request.input_payload.get("description", "")),
                    implementation_notes=str(request.input_payload.get("implementation_notes", "")),
                    name=str(request.input_payload.get("asset_name", "")),
                    project_root=project_root,
                    stream_callback=lambda chunk: ws.send_text(
                        json.dumps(
                            {
                                "event": "item_agent_stream",
                                "item_id": str(request.job_item_id),
                                "chunk": chunk,
                            }
                        )
                    ),
                    skip_build=True,
                )
                return StepExecutionResult(
                    step_id=request.step_id,
                    status="succeeded",
                    output_payload={"agent_output": output},
                )

            runner = WorkflowRunner(dispatcher=StepDispatcher(execute_handler))
            orchestrator = ExecutionOrchestratorService(
                job_repository=job_repository,
                ai_execution_repository=AIExecutionRepositorySqlAlchemy(session),
                quota_billing_service=None,
                job_event_repository=event_repository,
                workflow_registry=registry,
                workflow_runner=runner,
            )

            refreshed = job_repository.find_by_id_for_user(job.id, user_id)
            await ws.send_text(
                json.dumps(
                    {
                        "event": "batch_started",
                        "items": [{"id": item.id, "item_index": item.item_index} for item in refreshed.items],
                    }
                )
            )

            succeeded_count = 0
            for plan_item, item in zip(plan.items, refreshed.items):
                results = await orchestrator.run_registered_steps(
                    job_type="batch_generate",
                    item_type="custom_code",
                    job_id=job.id,
                    job_item_id=item.id,
                    workflow_version=job.workflow_version,
                    step_protocol_version="v1",
                    result_schema_version="v1",
                    input_payload=dict(item.input_payload),
                )
                result = results[-1]
                refreshed = job_repository.find_by_id_for_user(job.id, user_id)
                db_item = next(entry for entry in refreshed.items if entry.id == item.id)
                db_item.finished_at = datetime.now(UTC)
                if result.status == "succeeded":
                    db_item.status = JobItemStatus.SUCCEEDED
                    succeeded_count += 1
                    await ws.send_text(
                        json.dumps(
                            {
                                "event": "item_done",
                                "item_id": db_item.id,
                                "success": True,
                            }
                        )
                    )
                    event_repository.append(
                        job_id=job.id,
                        user_id=user_id,
                        event_type="job.item.completed",
                        payload={"job_item_id": db_item.id, "status": db_item.status.value, "name": plan_item.name},
                        job_item_id=db_item.id,
                    )
                else:
                    db_item.status = JobItemStatus.FAILED_SYSTEM
                job_repository.save(refreshed)

            refreshed = job_repository.find_by_id_for_user(job.id, user_id)
            refreshed.pending_item_count = 0
            refreshed.running_item_count = 0
            refreshed.succeeded_item_count = succeeded_count
            refreshed.failed_system_item_count = len(refreshed.items) - succeeded_count
            refreshed.status = JobStatus.SUCCEEDED if succeeded_count == len(refreshed.items) else JobStatus.FAILED
            refreshed.finished_at = datetime.now(UTC)
            if refreshed.status == JobStatus.SUCCEEDED:
                event_repository.append(
                    job_id=job.id,
                    user_id=user_id,
                    event_type="job.completed",
                    payload={"status": refreshed.status.value},
                )
            job_repository.save(refreshed)
            session.commit()

            await ws.send_text(
                json.dumps(
                    {
                        "event": "batch_done",
                        "success_count": succeeded_count,
                        "error_count": len(refreshed.items) - succeeded_count,
                    }
                )
            )
        except Exception as exc:
            session.rollback()
            try:
                await send_ws_error(
                    ws,
                    code="batch_workflow_failed",
                    message=str(exc),
                    detail=str(exc),
                    traceback=traceback.format_exc(),
                )
            except Exception:
                pass
        finally:
            session.close()
