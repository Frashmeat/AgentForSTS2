from __future__ import annotations

import json
import traceback
from datetime import UTC, datetime
from pathlib import Path
from typing import Awaitable, Callable

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


class WorkflowRouterCompatService:
    def __init__(
        self,
        *,
        session_factory=None,
        create_custom_code_fn: CodegenFn | None = None,
    ) -> None:
        self.session_factory = session_factory
        self.create_custom_code_fn = create_custom_code_fn

    async def handle_ws_create(self, ws) -> None:
        if self.session_factory is None:
            from routers import workflow as workflow_router

            await workflow_router._handle_legacy_ws_create(ws)
            return

        await ws.accept()
        raw = await ws.receive_text()
        params = json.loads(raw)
        if params.get("action") == "start" and params.get("asset_type") == "custom_code":
            await self._handle_platform_custom_code(ws, params)
            return

        from routers import workflow as workflow_router

        await workflow_router._handle_legacy_ws_create(ws, initial_params=params)

    async def _handle_platform_custom_code(self, ws, params: dict) -> None:
        project_root = Path(params["project_root"])
        user_id = int(params.get("user_id", 1001))
        create_custom_code = self.create_custom_code_fn
        if create_custom_code is None:
            from agents.code_agent import create_custom_code as default_create_custom_code

            create_custom_code = default_create_custom_code

        async def stream_callback(chunk: str) -> None:
            await ws.send_text(json.dumps({"event": "agent_stream", "chunk": chunk}))

        async def execute_handler(request) -> StepExecutionResult:
            output = await create_custom_code(
                description=str(request.input_payload.get("description", "")),
                implementation_notes=str(request.input_payload.get("implementation_notes", "")),
                name=str(request.input_payload.get("asset_name", "")),
                project_root=project_root,
                stream_callback=stream_callback,
                skip_build=True,
            )
            return StepExecutionResult(
                step_id=request.step_id,
                status="succeeded",
                output_payload={"agent_output": output},
            )

        session = self.session_factory()
        try:
            job_repository = JobRepositorySqlAlchemy(session)
            event_repository = JobEventRepositorySqlAlchemy(session)
            application_service = JobApplicationService(job_repository, event_repository)

            job = application_service.create_job(
                user_id=user_id,
                command=CreateJobCommand(
                    job_type="single_generate",
                    workflow_version="2026.03.31",
                    input_summary=params.get("description", ""),
                    items=[
                        CreateJobItemInput(
                            item_type="custom_code",
                            input_summary=params.get("description", ""),
                            input_payload={
                                "asset_name": params.get("asset_name", ""),
                                "description": params.get("description", ""),
                                "implementation_notes": params.get("implementation_notes", ""),
                                "project_root": str(project_root),
                            },
                        )
                    ],
                ),
            )
            started = application_service.start_job(
                user_id=user_id,
                command=StartJobCommand(job_id=job.id, triggered_by="platform_runner"),
            )
            if started is None:
                raise RuntimeError("failed to start platform job")

            registry = PlatformWorkflowRegistry(
                {
                    ("single_generate", "custom_code"): [
                        PlatformWorkflowStep(step_type="code.generate", step_id="code.generate")
                    ]
                }
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

            item = job_repository.find_by_id_for_user(job.id, user_id).items[0]
            results = await orchestrator.run_registered_steps(
                job_type="single_generate",
                item_type="custom_code",
                job_id=job.id,
                job_item_id=item.id,
                workflow_version=job.workflow_version,
                step_protocol_version="v1",
                result_schema_version="v1",
                input_payload=dict(item.input_payload),
            )
            result = results[-1]

            job = job_repository.find_by_id_for_user(job.id, user_id)
            item = job.items[0]
            job.pending_item_count = 0
            job.running_item_count = 0
            if result.status == "succeeded":
                item.status = JobItemStatus.SUCCEEDED
                job.status = JobStatus.SUCCEEDED
                job.succeeded_item_count = 1
                job.result_summary = str(result.output_payload.get("agent_output", ""))
                event_repository.append(
                    job_id=job.id,
                    user_id=user_id,
                    event_type="job.item.completed",
                    payload={"job_item_id": item.id, "status": item.status.value},
                    job_item_id=item.id,
                )
                event_repository.append(
                    job_id=job.id,
                    user_id=user_id,
                    event_type="job.completed",
                    payload={"status": job.status.value},
                )
            else:
                item.status = JobItemStatus.FAILED_SYSTEM
                job.status = JobStatus.FAILED
                job.failed_system_item_count = 1
                job.error_summary = result.error_summary
            item.finished_at = datetime.now(UTC)
            job.finished_at = datetime.now(UTC)
            job_repository.save(job)
            session.commit()

            await ws.send_text(
                json.dumps(
                    {
                        "event": "done",
                        "success": result.status == "succeeded",
                        "image_paths": [],
                        "agent_output": result.output_payload.get("agent_output", ""),
                        "job_id": job.id,
                    }
                )
            )
        except Exception as exc:
            session.rollback()
            try:
                await send_ws_error(
                    ws,
                    code="workflow_runtime_error",
                    message=str(exc),
                    detail=str(exc),
                    traceback=traceback.format_exc(),
                )
            except Exception:
                pass
        finally:
            session.close()

    async def create_project(self, body: dict):
        from routers import workflow as workflow_router

        return await workflow_router._legacy_api_create_project(body)

    async def build_project(self, body: dict):
        from routers import workflow as workflow_router

        return await workflow_router._legacy_api_build(body)

    async def package_project(self, body: dict):
        from routers import workflow as workflow_router

        return await workflow_router._legacy_api_package(body)
