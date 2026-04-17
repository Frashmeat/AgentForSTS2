from __future__ import annotations

from fastapi import Request

from app.modules.platform.application.services import (
    ExecutionOrchestratorService,
    ExecutionRoutingService,
    QuotaBillingService,
    ServerCredentialCipher,
)
from app.modules.platform.runner import ExecutionAdapter, PlatformWorkflowRegistry, PlatformWorkflowStep, StepDispatcher, WorkflowRunner
from app.modules.platform.runner.batch_custom_code_handler import execute_batch_custom_code_step
from app.modules.platform.runner.log_analysis_handler import execute_log_analysis_step
from app.modules.platform.runner.single_asset_plan_handler import execute_single_asset_plan_step
from app.modules.platform.runner.text_generate_handler import execute_text_generate_step


def build_execution_orchestrator_service(session, request: Request) -> ExecutionOrchestratorService:
    container = request.app.state.container
    job_repository = container.resolve_singleton("platform.job_repository_factory")(session)
    ai_execution_repository = container.resolve_singleton("platform.ai_execution_repository_factory")(session)
    job_event_repository = container.resolve_singleton("platform.job_event_repository_factory")(session)
    quota_billing_service = _build_quota_billing_service(session, request)
    execution_routing_service = _build_execution_routing_service(session, request)
    server_credential_cipher = _build_server_credential_cipher(request)
    return container.resolve_singleton("platform.execution_orchestrator_service_factory")(
        job_repository=job_repository,
        ai_execution_repository=ai_execution_repository,
        quota_billing_service=quota_billing_service,
        job_event_repository=job_event_repository,
        execution_routing_service=execution_routing_service,
        server_credential_cipher=server_credential_cipher,
        workflow_registry=_build_workflow_registry(request),
        workflow_runner=_build_workflow_runner(request),
    )


def _build_quota_billing_service(session, request: Request) -> QuotaBillingService:
    container = request.app.state.container
    execution_charge_repository = container.resolve_singleton("platform.execution_charge_repository_factory")(session)
    quota_account_repository = container.resolve_singleton("platform.quota_account_repository_factory")(session)
    usage_ledger_repository = container.resolve_singleton("platform.usage_ledger_repository_factory")(session)
    return container.resolve_singleton("platform.quota_billing_service_factory")(
        execution_charge_repository=execution_charge_repository,
        quota_account_repository=quota_account_repository,
        usage_ledger_repository=usage_ledger_repository,
    )


def _build_execution_routing_service(session, request: Request) -> ExecutionRoutingService:
    container = request.app.state.container
    repository = container.resolve_singleton("platform.execution_routing_repository_factory")(session)
    return container.resolve_singleton("platform.execution_routing_service_factory")(
        execution_routing_repository=repository,
    )


def _build_server_credential_cipher(request: Request) -> ServerCredentialCipher:
    container = request.app.state.container
    settings = container.resolve_singleton("settings")
    return container.resolve_singleton("platform.server_credential_cipher_factory").from_settings(settings)


def _build_workflow_registry(request: Request) -> PlatformWorkflowRegistry:
    container = request.app.state.container
    registry = container.resolve_singleton("platform.workflow_registry_factory")()
    registry.register(
        "log_analysis",
        "log_analysis",
        [PlatformWorkflowStep(step_type="log.analyze", step_id="log.analyze")],
    )
    registry.register(
        "batch_generate",
        "custom_code",
        [PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="batch.custom_code.plan")],
    )
    registry.register(
        "single_generate",
        "custom_code",
        [PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="single.custom_code.plan")],
    )
    registry.register(
        "single_generate",
        "relic",
        [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single.relic.plan")],
    )
    return registry


def _build_workflow_runner(request: Request) -> WorkflowRunner:
    container = request.app.state.container
    dispatcher = _build_step_dispatcher(request)
    return container.resolve_singleton("platform.workflow_runner_factory")(dispatcher=dispatcher)


def _build_step_dispatcher(request: Request) -> StepDispatcher:
    container = request.app.state.container
    adapter = _build_execution_adapter(request)
    return container.resolve_singleton("platform.step_dispatcher_factory")(execute_handler=adapter.execute)


def _build_execution_adapter(request: Request) -> ExecutionAdapter:
    container = request.app.state.container
    return container.resolve_singleton("platform.execution_adapter_factory")(
        image_handler=None,
        code_handler=None,
        text_handler=execute_text_generate_step,
        batch_custom_code_handler=execute_batch_custom_code_step,
        single_asset_plan_handler=execute_single_asset_plan_step,
        log_handler=execute_log_analysis_step,
        build_handler=None,
        approval_handler=None,
    )
