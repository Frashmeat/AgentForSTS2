from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_routing_service import ExecutionRoutingService
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence.models import ExecutionProfileRecord, ServerCredentialRecord
from app.modules.platform.infra.persistence.repositories.execution_routing_repository_sqlalchemy import (
    ExecutionRoutingRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy


def _seed_job_and_profile(db_session):
    profile = ExecutionProfileRecord(
        code="codex-gpt-5-4",
        display_name="Codex CLI / gpt-5.4",
        agent_backend="codex",
        model="gpt-5.4",
        description="默认推荐",
        enabled=True,
        recommended=True,
        sort_order=10,
    )
    db_session.add(profile)
    db_session.flush()
    credential_a = ServerCredentialRecord(
        execution_profile_id=profile.id,
        provider="openai",
        auth_type="api_key",
        credential_ciphertext="cipher-primary",
        secret_ciphertext=None,
        base_url="https://api-a.example.com/v1",
        label="primary",
        priority=5,
        enabled=True,
        health_status="healthy",
        last_checked_at=None,
        last_error_code="",
        last_error_message="",
    )
    credential_b = ServerCredentialRecord(
        execution_profile_id=profile.id,
        provider="openai",
        auth_type="api_key",
        credential_ciphertext="cipher-secondary",
        secret_ciphertext=None,
        base_url="https://api-b.example.com/v1",
        label="secondary",
        priority=10,
        enabled=True,
        health_status="healthy",
        last_checked_at=None,
        last_error_code="",
        last_error_message="",
    )
    db_session.add_all([credential_a, credential_b])
    db_session.flush()
    job = JobRepositorySqlAlchemy(db_session).create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [{"item_type": "card"}],
            }
        ),
    )
    db_session.flush()
    return job, profile, credential_a, credential_b


def test_execution_routing_service_can_resolve_retry_route_with_alternate_credential(db_session):
    job, profile, credential_a, credential_b = _seed_job_and_profile(db_session)
    service = ExecutionRoutingService(
        execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
    )

    route = service.resolve_retry_for_job(
        job,
        failed_credential_ref=f"server-credential:{credential_a.id}",
    )

    assert route.execution_profile_id == profile.id
    assert route.credential_ref == f"server-credential:{credential_b.id}"
    assert route.base_url == "https://api-b.example.com/v1"
    assert route.retry_attempt == 1
    assert route.switched_credential is True


def test_execution_routing_service_raises_when_retry_ref_is_invalid(db_session):
    job, _, _, _ = _seed_job_and_profile(db_session)
    service = ExecutionRoutingService(
        execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
    )

    try:
        service.resolve_retry_for_job(job, failed_credential_ref="cred-a")
    except ValueError as error:
        assert str(error) == "credential_ref is invalid for retry routing: cred-a"
    else:
        raise AssertionError("expected ValueError for invalid credential_ref")


def test_execution_routing_service_raises_when_no_alternate_credential_exists(db_session):
    job, profile, credential_a, credential_b = _seed_job_and_profile(db_session)
    credential_b.health_status = "rate_limited"
    db_session.flush()
    service = ExecutionRoutingService(
        execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
    )

    try:
        service.resolve_retry_for_job(
            job,
            failed_credential_ref=f"server-credential:{credential_a.id}",
        )
    except LookupError as error:
        assert str(error) == f"no alternate enabled healthy server credential for execution profile: {profile.id}"
    else:
        raise AssertionError("expected LookupError when no alternate credential exists")
