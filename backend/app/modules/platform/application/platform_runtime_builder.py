from __future__ import annotations

from functools import partial
from typing import Any

from app.modules.platform.application.services import (
    ExecutionOrchestratorService,
    ExecutionRoutingService,
    JobApplicationService,
    QuotaBillingService,
    ServerCredentialCipher,
    ServerDeployTargetLockService,
    ServerQueuedJobClaimService,
    ServerWorkspaceLockService,
    ServerWorkspaceService,
    UploadedAssetService,
)
from app.modules.platform.application.workstation_execution_client import WorkstationExecutionClient
from app.modules.platform.runner import (
    ExecutionAdapter,
    PlatformWorkflowRegistry,
    PlatformWorkflowStep,
    StepDispatcher,
    WorkflowRunner,
)
from app.modules.platform.runner.asset_generate_handler import execute_asset_generate_step
from app.modules.platform.runner.batch_custom_code_handler import execute_batch_custom_code_step
from app.modules.platform.runner.build_project_handler import execute_build_project_step
from app.modules.platform.runner.code_generate_handler import execute_code_generate_step
from app.modules.platform.runner.log_analysis_handler import execute_log_analysis_step
from app.modules.platform.runner.single_asset_plan_handler import execute_single_asset_plan_step
from app.modules.platform.runner.text_generate_handler import execute_text_generate_step


def build_job_application_service_from_container(session, container: Any) -> JobApplicationService:
    job_repository = container.resolve_singleton("platform.job_repository_factory")(session)
    job_event_repository = container.resolve_singleton("platform.job_event_repository_factory")(session)
    return container.resolve_singleton("platform.job_application_service_factory")(
        job_repository=job_repository,
        job_event_repository=job_event_repository,
        execution_orchestrator_service=build_execution_orchestrator_service_from_container(session, container),
        server_queued_job_claim_service=_build_server_queued_job_claim_service_from_container(container),
        server_workspace_service=_build_server_workspace_service_from_container(container),
        uploaded_asset_service=_build_uploaded_asset_service_from_container(container),
    )


def build_execution_orchestrator_service_from_container(session, container: Any) -> ExecutionOrchestratorService:
    job_repository = container.resolve_singleton("platform.job_repository_factory")(session)
    ai_execution_repository = container.resolve_singleton("platform.ai_execution_repository_factory")(session)
    artifact_repository = container.resolve_singleton("platform.artifact_repository_factory")(session)
    server_credential_admin_repository = container.resolve_singleton(
        "platform.server_credential_admin_repository_factory"
    )(session)
    job_event_repository = container.resolve_singleton("platform.job_event_repository_factory")(session)
    quota_billing_service = _build_quota_billing_service_from_container(session, container)
    execution_routing_service = _build_execution_routing_service_from_container(session, container)
    server_credential_cipher = _build_server_credential_cipher_from_container(container)
    server_workspace_lock_service = _build_server_workspace_lock_service_from_container(container)
    server_workspace_service = _build_server_workspace_service_from_container(container)
    uploaded_asset_service = _build_uploaded_asset_service_from_container(container)
    return container.resolve_singleton("platform.execution_orchestrator_service_factory")(
        job_repository=job_repository,
        ai_execution_repository=ai_execution_repository,
        artifact_repository=artifact_repository,
        quota_billing_service=quota_billing_service,
        job_event_repository=job_event_repository,
        execution_routing_service=execution_routing_service,
        server_credential_cipher=server_credential_cipher,
        server_workspace_lock_service=server_workspace_lock_service,
        server_workspace_service=server_workspace_service,
        uploaded_asset_service=uploaded_asset_service,
        workflow_registry=_build_workflow_registry_from_container(container),
        workflow_runner=_build_workflow_runner_from_container(container),
        server_credential_admin_repository=server_credential_admin_repository,
        workstation_execution_client=_build_workstation_execution_client_from_container(container),
    )


def _build_quota_billing_service_from_container(session, container: Any) -> QuotaBillingService:
    execution_charge_repository = container.resolve_singleton("platform.execution_charge_repository_factory")(session)
    quota_account_repository = container.resolve_singleton("platform.quota_account_repository_factory")(session)
    quota_balance_repository = container.resolve_singleton("platform.quota_balance_repository_factory")(session)
    usage_ledger_repository = container.resolve_singleton("platform.usage_ledger_repository_factory")(session)
    return container.resolve_singleton("platform.quota_billing_service_factory")(
        execution_charge_repository=execution_charge_repository,
        quota_account_repository=quota_account_repository,
        usage_ledger_repository=usage_ledger_repository,
        quota_balance_repository=quota_balance_repository,
    )


def _build_execution_routing_service_from_container(session, container: Any) -> ExecutionRoutingService:
    repository = container.resolve_singleton("platform.execution_routing_repository_factory")(session)
    return container.resolve_singleton("platform.execution_routing_service_factory")(
        execution_routing_repository=repository,
    )


def _build_server_credential_cipher_from_container(container: Any) -> ServerCredentialCipher:
    settings = container.resolve_singleton("settings")
    return container.resolve_singleton("platform.server_credential_cipher_factory").from_settings(settings)


def _build_uploaded_asset_service_from_container(container: Any) -> UploadedAssetService:
    factory = container.resolve_singleton("platform.uploaded_asset_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_server_workspace_service_from_container(container: Any) -> ServerWorkspaceService:
    factory = container.resolve_singleton("platform.server_workspace_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_server_workspace_lock_service_from_container(container: Any) -> ServerWorkspaceLockService:
    factory = container.resolve_singleton("platform.server_workspace_lock_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_server_queued_job_claim_service_from_container(container: Any) -> ServerQueuedJobClaimService:
    factory = container.resolve_singleton("platform.server_queued_job_claim_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_workstation_execution_client_from_container(container: Any) -> WorkstationExecutionClient | None:
    if container.resolve_optional_singleton("runtime_role") != "web":
        return None
    settings = container.resolve_singleton("settings")
    return WorkstationExecutionClient(
        settings=settings,
        runtime_controller=container.resolve_optional_singleton("platform.workstation_runtime_manager"),
    )


def _build_server_deploy_target_lock_service_from_container(container: Any) -> ServerDeployTargetLockService:
    factory = container.resolve_singleton("platform.server_deploy_target_lock_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_workflow_registry_from_container(container: Any) -> PlatformWorkflowRegistry:
    registry = container.resolve_singleton("platform.workflow_registry_factory")()

    def resolve_single_card_fullscreen(input_payload: dict[str, object]) -> list[PlatformWorkflowStep]:
        uploaded_asset_ref = str(input_payload.get("uploaded_asset_ref", "")).strip()
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if uploaded_asset_ref and server_project_ref:
            return [
                PlatformWorkflowStep(
                    step_type="asset.generate",
                    step_id="single.card_fullscreen.asset",
                    input_payload={"asset_type": "card_fullscreen"},
                ),
                PlatformWorkflowStep(step_type="build.project", step_id="single.card_fullscreen.build"),
            ]
        return [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="single.card_fullscreen.plan",
                input_payload={"asset_type": "card_fullscreen"},
            )
        ]

    def resolve_batch_card_fullscreen(input_payload: dict[str, object]) -> list[PlatformWorkflowStep]:
        uploaded_asset_ref = str(input_payload.get("uploaded_asset_ref", "")).strip()
        server_project_ref = str(input_payload.get("server_project_ref", "")).strip()
        if uploaded_asset_ref and server_project_ref:
            return [
                PlatformWorkflowStep(
                    step_type="asset.generate",
                    step_id="batch.card_fullscreen.asset",
                    input_payload={"asset_type": "card_fullscreen"},
                ),
                PlatformWorkflowStep(step_type="build.project", step_id="batch.card_fullscreen.build"),
            ]
        return [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="batch.card_fullscreen.plan",
                input_payload={"asset_type": "card_fullscreen"},
            )
        ]

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
    registry.register("batch_generate", "card_fullscreen", resolve_batch_card_fullscreen)
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
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="single.card.plan",
                input_payload={"asset_type": "card"},
            )
        ],
    )
    registry.register("single_generate", "card_fullscreen", resolve_single_card_fullscreen)
    registry.register(
        "single_generate",
        "relic",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="single.relic.plan",
                input_payload={"asset_type": "relic"},
            )
        ],
    )
    registry.register(
        "single_generate",
        "power",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="single.power.plan",
                input_payload={"asset_type": "power"},
            )
        ],
    )
    registry.register(
        "single_generate",
        "character",
        [
            PlatformWorkflowStep(
                step_type="single.asset.plan",
                step_id="single.character.plan",
                input_payload={"asset_type": "character"},
            )
        ],
    )
    return registry


def _build_workflow_runner_from_container(container: Any) -> WorkflowRunner:
    dispatcher = _build_step_dispatcher_from_container(container)
    return container.resolve_singleton("platform.workflow_runner_factory")(dispatcher=dispatcher)


def _build_step_dispatcher_from_container(container: Any) -> StepDispatcher:
    adapter = _build_execution_adapter_from_container(container)
    return container.resolve_singleton("platform.step_dispatcher_factory")(execute_handler=adapter.execute)


def _build_execution_adapter_from_container(container: Any) -> ExecutionAdapter:
    return container.resolve_singleton("platform.execution_adapter_factory")(
        image_handler=None,
        code_handler=execute_code_generate_step,
        asset_handler=execute_asset_generate_step,
        text_handler=execute_text_generate_step,
        batch_custom_code_handler=execute_batch_custom_code_step,
        single_asset_plan_handler=execute_single_asset_plan_step,
        log_handler=execute_log_analysis_step,
        build_handler=partial(
            execute_build_project_step,
            deploy_target_lock_service=_build_server_deploy_target_lock_service_from_container(container),
        ),
        approval_handler=None,
    )
