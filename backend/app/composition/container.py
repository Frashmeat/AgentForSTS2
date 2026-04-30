from __future__ import annotations

from typing import Any, Literal

from app.modules.auth.application import AuthService, PBKDF2PasswordHasher
from app.modules.auth.infra.persistence.repositories import (
    EmailVerificationRepositorySqlAlchemy,
    UserRepositorySqlAlchemy,
)
from app.modules.platform.application.services import (
    AdminQueryService,
    AdminQuotaCommandService,
    EventService,
    ExecutionOrchestratorService,
    ExecutionRoutingService,
    JobApplicationService,
    JobQueryService,
    PlatformRequestRateLimiter,
    PlatformRuntimeAuditService,
    QuotaBillingService,
    ServerCredentialAdminService,
    ServerCredentialCipher,
    ServerCredentialHealthChecker,
    ServerDeployTargetLockService,
    ServerExecutionService,
    ServerQueuedJobClaimService,
    ServerQueuedJobScanClaimService,
    ServerWorkspaceLockService,
    ServerWorkspaceService,
    UploadedAssetService,
)
from app.modules.platform.infra.persistence.repositories import (
    AdminQueryRepositoriesSqlAlchemy,
    AIExecutionRepositorySqlAlchemy,
    ArtifactRepositorySqlAlchemy,
    ExecutionChargeRepositorySqlAlchemy,
    ExecutionRoutingRepositorySqlAlchemy,
    JobEventRepositorySqlAlchemy,
    JobQueryRepositorySqlAlchemy,
    JobRepositorySqlAlchemy,
    QuotaAccountRepositorySqlAlchemy,
    QuotaBalanceRepositorySqlAlchemy,
    QuotaQueryRepositorySqlAlchemy,
    ServerCredentialAdminRepositorySqlAlchemy,
    ServerExecutionRepositorySqlAlchemy,
    UsageLedgerRepositorySqlAlchemy,
)
from app.modules.platform.runner import (
    ApprovalAdapter,
    BuildDeployAdapter,
    ExecutionAdapter,
    PlatformWorkflowRegistry,
    StepDispatcher,
    WorkflowRunner,
)
from app.shared.infra.config.settings import Settings
from app.shared.infra.db.session import create_session_factory

from .registry import ProviderRegistry

RuntimeRole = Literal["workstation", "web"]


class ApplicationContainer:
    def __init__(
        self,
        settings: Settings,
        registry: ProviderRegistry | None = None,
        runtime_role: RuntimeRole = "workstation",
    ) -> None:
        self.settings = settings
        self.registry = registry or ProviderRegistry()
        self.runtime_role = runtime_role
        self._singletons: dict[str, Any] = {}
        self._bootstrap_defaults()

    @classmethod
    def from_config(
        cls,
        config: dict[str, Any] | None,
        runtime_role: RuntimeRole = "workstation",
    ) -> ApplicationContainer:
        return cls(settings=Settings.from_dict(config), runtime_role=runtime_role)

    def _bootstrap_defaults(self) -> None:
        database_url = str(self.settings.database.get("url", "")).strip()
        db_session_factory = create_session_factory(self.settings.database) if database_url else None

        self._singletons.setdefault("settings", self.settings)
        self._singletons.setdefault("runtime_role", self.runtime_role)
        self._singletons.setdefault("platform.db_session_factory", db_session_factory)
        self._singletons.setdefault("auth.db_session_factory", db_session_factory)
        self._bootstrap_platform_defaults(db_session_factory)
        self._bootstrap_auth_defaults()
        if self.runtime_role != "web":
            self._bootstrap_workstation_bridge_defaults(db_session_factory)
        for key in (
            "platform.job_service",
            "platform.job_query_service",
            "platform.admin_query_service",
            "platform.runner",
        ):
            self._singletons.setdefault(key, None)

    def _bootstrap_platform_defaults(self, db_session_factory: Any) -> None:
        for key, instance in (
            ("platform.job_repository_factory", JobRepositorySqlAlchemy),
            ("platform.job_query_repository_factory", JobQueryRepositorySqlAlchemy),
            ("platform.quota_query_repository_factory", QuotaQueryRepositorySqlAlchemy),
            ("platform.ai_execution_repository_factory", AIExecutionRepositorySqlAlchemy),
            ("platform.execution_routing_repository_factory", ExecutionRoutingRepositorySqlAlchemy),
            ("platform.execution_charge_repository_factory", ExecutionChargeRepositorySqlAlchemy),
            ("platform.quota_account_repository_factory", QuotaAccountRepositorySqlAlchemy),
            ("platform.quota_balance_repository_factory", QuotaBalanceRepositorySqlAlchemy),
            ("platform.usage_ledger_repository_factory", UsageLedgerRepositorySqlAlchemy),
            ("platform.artifact_repository_factory", ArtifactRepositorySqlAlchemy),
            ("platform.job_event_repository_factory", JobEventRepositorySqlAlchemy),
            ("platform.admin_query_repositories_factory", AdminQueryRepositoriesSqlAlchemy),
            ("platform.server_credential_admin_repository_factory", ServerCredentialAdminRepositorySqlAlchemy),
            ("platform.server_execution_repository_factory", ServerExecutionRepositorySqlAlchemy),
            ("platform.job_application_service_factory", JobApplicationService),
            ("platform.job_query_service_factory", JobQueryService),
            ("platform.request_rate_limiter", PlatformRequestRateLimiter()),
            ("platform.admin_query_service_factory", AdminQueryService),
            ("platform.admin_quota_command_service_factory", AdminQuotaCommandService),
            ("platform.runtime_audit_service_factory", PlatformRuntimeAuditService),
            ("platform.server_credential_admin_service_factory", ServerCredentialAdminService),
            ("platform.server_credential_cipher_factory", ServerCredentialCipher),
            ("platform.server_credential_health_checker_factory", ServerCredentialHealthChecker),
            ("platform.server_queued_job_claim_service_factory", ServerQueuedJobClaimService),
            ("platform.server_queued_job_scan_claim_service_factory", ServerQueuedJobScanClaimService),
            ("platform.server_deploy_target_lock_service_factory", ServerDeployTargetLockService),
            ("platform.server_execution_service_factory", ServerExecutionService),
            ("platform.server_workspace_lock_service_factory", ServerWorkspaceLockService),
            ("platform.server_workspace_service_factory", ServerWorkspaceService),
            ("platform.uploaded_asset_service_factory", UploadedAssetService),
            ("platform.execution_orchestrator_service_factory", ExecutionOrchestratorService),
            ("platform.quota_billing_service_factory", QuotaBillingService),
            ("platform.event_service_factory", EventService),
            ("platform.execution_routing_service_factory", ExecutionRoutingService),
            ("platform.workflow_registry_factory", PlatformWorkflowRegistry),
            ("platform.step_dispatcher_factory", StepDispatcher),
            ("platform.execution_adapter_factory", ExecutionAdapter),
            ("platform.workflow_runner_factory", WorkflowRunner),
        ):
            self._singletons.setdefault(key, instance)

    def _bootstrap_auth_defaults(self) -> None:
        for key, instance in (
            ("auth.user_repository_factory", UserRepositorySqlAlchemy),
            ("auth.email_verification_repository_factory", EmailVerificationRepositorySqlAlchemy),
            ("auth.auth_service_factory", AuthService),
            ("auth.password_hasher_factory", PBKDF2PasswordHasher),
        ):
            self._singletons.setdefault(key, instance)

    def _bootstrap_workstation_bridge_defaults(self, db_session_factory: Any) -> None:
        for key, instance in (
            ("platform.build_deploy_adapter_factory", BuildDeployAdapter),
            ("platform.approval_adapter_factory", ApprovalAdapter),
        ):
            self._singletons.setdefault(key, instance)

    def register_singleton(self, key: str, instance: Any) -> None:
        self._singletons[key] = instance

    def resolve_singleton(self, key: str) -> Any:
        return self._singletons[key]

    def has_singleton(self, key: str) -> bool:
        return key in self._singletons

    def resolve_optional_singleton(self, key: str, default: Any = None) -> Any:
        return self._singletons.get(key, default)

    def register_provider(self, kind: str, name: str, provider: Any) -> None:
        self.registry.register(kind, name, provider)

    def resolve_provider(self, kind: str, name: str) -> Any:
        return self.registry.get(kind, name)
