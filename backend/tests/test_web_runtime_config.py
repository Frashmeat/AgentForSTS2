from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.composition.container import ApplicationContainer
from app.shared.infra.config.settings import Settings


def test_settings_expose_web_runtime_defaults_and_validation():
    settings = Settings.from_dict(None)

    runtime = settings.get_runtime("web")

    assert runtime["port"] == 7870
    assert runtime["mount_frontend"] is False
    assert runtime["requires_database"] is True
    assert "http://localhost:7870" in runtime["cors_origins"]
    assert "http://127.0.0.1:8080" in runtime["cors_origins"]
    assert settings.validate_for_role("web") == [
        "database.url is required for web runtime",
        "auth.session_secret is required for web runtime",
    ]


def test_settings_expose_workstation_runtime_origins_for_independent_frontend():
    settings = Settings.from_dict(None)

    runtime = settings.get_runtime("workstation")

    assert runtime["port"] == 7860
    assert runtime["mount_frontend"] is True
    assert runtime["requires_database"] is False
    assert "http://localhost:8080" in runtime["cors_origins"]
    assert "http://127.0.0.1:8080" in runtime["cors_origins"]


def test_web_runtime_validation_passes_when_database_and_session_secret_are_configured():
    settings = Settings.from_dict(
        {
            "database": {
                "url": "sqlite+pysqlite:///:memory:",
            },
            "auth": {
                "session_secret": "test-session-secret",
            },
        }
    )

    assert settings.validate_for_role("web") == []


def test_web_runtime_container_exposes_minimal_platform_runner_singletons():
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": "sqlite+pysqlite:///:memory:",
            }
        },
        runtime_role="web",
    )

    assert container.runtime_role == "web"
    assert container.has_singleton("platform.job_repository_factory") is True
    assert container.has_singleton("platform.job_query_service_factory") is True
    assert container.has_singleton("platform.admin_query_service_factory") is True
    assert container.has_singleton("platform.server_credential_admin_service_factory") is True
    assert container.has_singleton("platform.server_credential_cipher_factory") is True
    assert container.has_singleton("platform.config_facade_service_factory") is False
    assert container.has_singleton("platform.workflow_registry_factory") is True
    assert container.has_singleton("platform.step_dispatcher_factory") is True
    assert container.has_singleton("platform.execution_adapter_factory") is True
    assert container.has_singleton("platform.workflow_runner_factory") is True
    assert container.has_singleton("platform.approval_adapter_factory") is False
