import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.composition.container import ApplicationContainer
from app.shared.infra.feature_flags import PlatformMigrationFlags, WorkflowMigrationFlags


def test_container_bootstraps_platform_placeholders_and_flags():
    container = ApplicationContainer.from_config(
        {
            "migration": {
                "platform_jobs_api_enabled": True,
                "platform_events_v1_enabled": True,
            }
        }
    )

    assert container.has_singleton("settings") is True
    assert container.has_singleton("platform.db_session_factory") is True
    assert container.resolve_optional_singleton("platform.db_session_factory") is None
    assert isinstance(container.workflow_migration_flags, WorkflowMigrationFlags)
    assert isinstance(container.platform_migration_flags, PlatformMigrationFlags)
    assert container.platform_migration_flags.platform_jobs_api_enabled is True
    assert container.platform_migration_flags.platform_events_v1_enabled is True


def test_container_can_override_platform_placeholder_singletons():
    container = ApplicationContainer.from_config(None)
    fake_service = object()

    container.register_singleton("platform.job_service", fake_service)

    assert container.resolve_singleton("platform.job_service") is fake_service


def test_container_exposes_platform_repository_factories_when_database_is_configured():
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": "sqlite+pysqlite:///:memory:",
            }
        }
    )

    session_factory = container.resolve_optional_singleton("platform.db_session_factory")

    assert session_factory is not None
    assert callable(container.resolve_singleton("platform.job_repository_factory"))
    assert callable(container.resolve_singleton("platform.job_query_repository_factory"))
    assert callable(container.resolve_singleton("platform.admin_query_repositories_factory"))
    assert callable(container.resolve_singleton("platform.job_application_service_factory"))
    assert callable(container.resolve_singleton("platform.execution_orchestrator_service_factory"))
    assert callable(container.resolve_singleton("platform.job_query_service_factory"))
    assert callable(container.resolve_singleton("platform.workflow_registry_factory"))
    assert callable(container.resolve_singleton("platform.workflow_runner_factory"))
    assert callable(container.resolve_singleton("platform.execution_adapter_factory"))
