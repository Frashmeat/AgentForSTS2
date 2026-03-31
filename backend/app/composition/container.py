from __future__ import annotations

from typing import Any, Optional

from app.modules.platform.application.services import (
    AdminQueryService,
    ApprovalFacadeService,
    BuildDeployFacadeService,
    EventService,
    ExecutionOrchestratorService,
    JobApplicationService,
    JobQueryService,
    QuotaBillingService,
)
from app.modules.platform.infra.persistence.repositories import (
    AdminQueryRepositoriesSqlAlchemy,
    AIExecutionRepositorySqlAlchemy,
    ArtifactRepositorySqlAlchemy,
    ExecutionChargeRepositorySqlAlchemy,
    JobEventRepositorySqlAlchemy,
    JobQueryRepositorySqlAlchemy,
    JobRepositorySqlAlchemy,
    QuotaAccountRepositorySqlAlchemy,
    QuotaQueryRepositorySqlAlchemy,
    UsageLedgerRepositorySqlAlchemy,
)
from app.shared.infra.config.settings import Settings
from app.shared.infra.db.session import create_session_factory
from app.shared.infra.feature_flags import (
    PlatformMigrationFlags,
    WorkflowMigrationFlags,
    resolve_platform_migration_flags,
    resolve_workflow_migration_flags,
)

from .registry import ProviderRegistry


class ApplicationContainer:
    def __init__(self, settings: Settings, registry: Optional[ProviderRegistry] = None) -> None:
        self.settings = settings
        self.registry = registry or ProviderRegistry()
        self._singletons: dict[str, Any] = {}
        self._bootstrap_defaults()

    @classmethod
    def from_config(cls, config: Optional[dict[str, Any]]) -> "ApplicationContainer":
        return cls(settings=Settings.from_dict(config))

    def _bootstrap_defaults(self) -> None:
        database_url = str(self.settings.database.get("url", "")).strip()
        db_session_factory = create_session_factory(self.settings.database) if database_url else None

        self._singletons.setdefault("settings", self.settings)
        self._singletons.setdefault(
            "workflow_migration_flags",
            resolve_workflow_migration_flags(self.settings.to_dict()),
        )
        self._singletons.setdefault(
            "platform_migration_flags",
            resolve_platform_migration_flags(self.settings.to_dict()),
        )
        self._singletons.setdefault("platform.db_session_factory", db_session_factory)
        for key, instance in (
            ("platform.job_repository_factory", JobRepositorySqlAlchemy),
            ("platform.job_query_repository_factory", JobQueryRepositorySqlAlchemy),
            ("platform.quota_query_repository_factory", QuotaQueryRepositorySqlAlchemy),
            ("platform.ai_execution_repository_factory", AIExecutionRepositorySqlAlchemy),
            ("platform.execution_charge_repository_factory", ExecutionChargeRepositorySqlAlchemy),
            ("platform.quota_account_repository_factory", QuotaAccountRepositorySqlAlchemy),
            ("platform.usage_ledger_repository_factory", UsageLedgerRepositorySqlAlchemy),
            ("platform.artifact_repository_factory", ArtifactRepositorySqlAlchemy),
            ("platform.job_event_repository_factory", JobEventRepositorySqlAlchemy),
            ("platform.admin_query_repositories_factory", AdminQueryRepositoriesSqlAlchemy),
            ("platform.job_application_service_factory", JobApplicationService),
            ("platform.job_query_service_factory", JobQueryService),
            ("platform.admin_query_service_factory", AdminQueryService),
            ("platform.execution_orchestrator_service_factory", ExecutionOrchestratorService),
            ("platform.quota_billing_service_factory", QuotaBillingService),
            ("platform.event_service_factory", EventService),
            ("platform.approval_facade_service_factory", ApprovalFacadeService),
            ("platform.build_deploy_facade_service_factory", BuildDeployFacadeService),
        ):
            self._singletons.setdefault(key, instance)
        for key in (
            "platform.job_service",
            "platform.job_query_service",
            "platform.admin_query_service",
            "platform.runner",
        ):
            self._singletons.setdefault(key, None)

    def register_singleton(self, key: str, instance: Any) -> None:
        self._singletons[key] = instance

    def resolve_singleton(self, key: str) -> Any:
        return self._singletons[key]

    def has_singleton(self, key: str) -> bool:
        return key in self._singletons

    def resolve_optional_singleton(self, key: str, default: Any = None) -> Any:
        return self._singletons.get(key, default)

    @property
    def workflow_migration_flags(self) -> WorkflowMigrationFlags:
        return self.resolve_singleton("workflow_migration_flags")

    @property
    def platform_migration_flags(self) -> PlatformMigrationFlags:
        return self.resolve_singleton("platform_migration_flags")

    def register_provider(self, kind: str, name: str, provider: Any) -> None:
        self.registry.register(kind, name, provider)

    def resolve_provider(self, kind: str, name: str) -> Any:
        return self.registry.get(kind, name)
