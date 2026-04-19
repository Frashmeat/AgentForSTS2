from __future__ import annotations

from fastapi import Request

from app.modules.platform.application.services import (
    ExecutionOrchestratorService,
    ExecutionRoutingService,
    QuotaBillingService,
    ServerCredentialCipher,
    ServerWorkspaceService,
    UploadedAssetService,
)
from app.modules.platform.runner import ExecutionAdapter, PlatformWorkflowRegistry, PlatformWorkflowStep, StepDispatcher, WorkflowRunner
from app.modules.platform.runner.asset_generate_handler import execute_asset_generate_step
from app.modules.platform.runner.batch_custom_code_handler import execute_batch_custom_code_step
from app.modules.platform.runner.build_project_handler import execute_build_project_step
from app.modules.platform.runner.code_generate_handler import execute_code_generate_step
from app.modules.platform.runner.log_analysis_handler import execute_log_analysis_step
from app.modules.platform.runner.single_asset_plan_handler import execute_single_asset_plan_step
from app.modules.platform.runner.text_generate_handler import execute_text_generate_step


def build_execution_orchestrator_service(session, request: Request) -> ExecutionOrchestratorService:
    container = request.app.state.container
    job_repository = container.resolve_singleton("platform.job_repository_factory")(session)
    ai_execution_repository = container.resolve_singleton("platform.ai_execution_repository_factory")(session)
    artifact_repository = container.resolve_singleton("platform.artifact_repository_factory")(session)
    job_event_repository = container.resolve_singleton("platform.job_event_repository_factory")(session)
    quota_billing_service = _build_quota_billing_service(session, request)
    execution_routing_service = _build_execution_routing_service(session, request)
    server_credential_cipher = _build_server_credential_cipher(request)
    server_workspace_service = _build_server_workspace_service(request)
    uploaded_asset_service = _build_uploaded_asset_service(request)
    return container.resolve_singleton("platform.execution_orchestrator_service_factory")(
        job_repository=job_repository,
        ai_execution_repository=ai_execution_repository,
        artifact_repository=artifact_repository,
        quota_billing_service=quota_billing_service,
        job_event_repository=job_event_repository,
        execution_routing_service=execution_routing_service,
        server_credential_cipher=server_credential_cipher,
        server_workspace_service=server_workspace_service,
        uploaded_asset_service=uploaded_asset_service,
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


def _build_uploaded_asset_service(request: Request) -> UploadedAssetService:
    container = request.app.state.container
    factory = container.resolve_singleton("platform.uploaded_asset_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_server_workspace_service(request: Request) -> ServerWorkspaceService:
    container = request.app.state.container
    factory = container.resolve_singleton("platform.server_workspace_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_workflow_registry(request: Request) -> PlatformWorkflowRegistry:
    container = request.app.state.container
    registry = container.resolve_singleton("platform.workflow_registry_factory")()
    def resolve_single_card_fullscreen(input_payload: dict[str, object]) -> list[PlatformWorkflowStep]:
        uploaded_asset_ref = str(input_payload.get("uploaded_asset_ref", "")).strip()
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if uploaded_asset_ref and server_project_ref:
            return [
                PlatformWorkflowStep(step_type="asset.generate", step_id="single.card_fullscreen.asset"),
                PlatformWorkflowStep(step_type="build.project", step_id="single.card_fullscreen.build"),
            ]
        return [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single.card_fullscreen.plan")]

    registry.register(
        "log_analysis",
        "log_analysis",
        [PlatformWorkflowStep(step_type="log.analyze", step_id="log.analyze")],
    )
    registry.register(
        "batch_generate",
        "custom_code",
        [
            PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="batch.custom_code.plan"),
            PlatformWorkflowStep(step_type="code.generate", step_id="batch.custom_code.codegen"),
            PlatformWorkflowStep(step_type="build.project", step_id="batch.custom_code.build"),
        ],
    )
    registry.register(
        "batch_generate",
        "card",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="batch.card.plan",
                input_payload={"asset_type": "card"},
            )
        ],
    )
    registry.register(
        "batch_generate",
        "card_fullscreen",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="batch.card_fullscreen.plan",
                input_payload={"asset_type": "card_fullscreen"},
            )
        ],
    )
    registry.register(
        "batch_generate",
        "relic",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="batch.relic.plan",
                input_payload={"asset_type": "relic"},
            )
        ],
    )
    registry.register(
        "batch_generate",
        "power",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="batch.power.plan",
                input_payload={"asset_type": "power"},
            )
        ],
    )
    registry.register(
        "batch_generate",
        "character",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="batch.character.plan",
                input_payload={"asset_type": "character"},
            )
        ],
    )
    registry.register(
        "single_generate",
        "custom_code",
        [
            PlatformWorkflowStep(step_type="batch.custom_code.plan", step_id="single.custom_code.plan"),
            PlatformWorkflowStep(step_type="code.generate", step_id="single.custom_code.codegen"),
            PlatformWorkflowStep(step_type="build.project", step_id="single.custom_code.build"),
        ],
    )
    registry.register(
        "single_generate",
        "card",
        [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single.card.plan")],
    )
    registry.register(
        "single_generate",
        "card_fullscreen",
        resolve_single_card_fullscreen,
    )
    registry.register(
        "single_generate",
        "relic",
        [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single.relic.plan")],
    )
    registry.register(
        "single_generate",
        "power",
        [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single.power.plan")],
    )
    registry.register(
        "single_generate",
        "character",
        [PlatformWorkflowStep(step_type="single.asset.plan", step_id="single.character.plan")],
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
        code_handler=execute_code_generate_step,
        asset_handler=execute_asset_generate_step,
        text_handler=execute_text_generate_step,
        batch_custom_code_handler=execute_batch_custom_code_step,
        single_asset_plan_handler=execute_single_asset_plan_step,
        log_handler=execute_log_analysis_step,
        build_handler=execute_build_project_step,
        approval_handler=None,
    )
