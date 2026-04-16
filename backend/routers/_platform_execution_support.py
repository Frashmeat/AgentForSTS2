from __future__ import annotations

from fastapi import Request

from app.modules.platform.application.services import (
    ExecutionOrchestratorService,
    ExecutionRoutingService,
    QuotaBillingService,
    ServerCredentialCipher,
)


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
