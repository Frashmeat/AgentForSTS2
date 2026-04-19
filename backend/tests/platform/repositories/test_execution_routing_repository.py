from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.infra.persistence.models import ExecutionProfileRecord, ServerCredentialRecord
from app.modules.platform.infra.persistence.repositories.execution_routing_repository_sqlalchemy import (
    ExecutionRoutingRepositorySqlAlchemy,
)


def test_execution_routing_repository_returns_first_enabled_healthy_credential_by_priority(db_session):
    repository = ExecutionRoutingRepositorySqlAlchemy(db_session)
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
    db_session.add_all(
        [
            ServerCredentialRecord(
                execution_profile_id=profile.id,
                provider="openai",
                auth_type="api_key",
                credential_ciphertext="cipher-disabled",
                secret_ciphertext=None,
                base_url="https://disabled.example.com/v1",
                label="disabled",
                priority=1,
                enabled=False,
                health_status="healthy",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            ),
            ServerCredentialRecord(
                execution_profile_id=profile.id,
                provider="openai",
                auth_type="api_key",
                credential_ciphertext="cipher-degraded",
                secret_ciphertext=None,
                base_url="https://degraded.example.com/v1",
                label="degraded",
                priority=2,
                enabled=True,
                health_status="degraded",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            ),
            ServerCredentialRecord(
                execution_profile_id=profile.id,
                provider="openai",
                auth_type="api_key",
                credential_ciphertext="cipher-healthy-high-priority",
                secret_ciphertext=None,
                base_url="https://healthy-a.example.com/v1",
                label="healthy-a",
                priority=5,
                enabled=True,
                health_status="healthy",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            ),
            ServerCredentialRecord(
                execution_profile_id=profile.id,
                provider="openai",
                auth_type="api_key",
                credential_ciphertext="cipher-healthy-low-priority",
                secret_ciphertext=None,
                base_url="https://healthy-b.example.com/v1",
                label="healthy-b",
                priority=10,
                enabled=True,
                health_status="healthy",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            ),
        ]
    )
    db_session.commit()

    route = repository.find_routable_execution_target(profile.id)

    assert route is not None
    assert route.execution_profile_id == profile.id
    assert route.agent_backend == "codex"
    assert route.model == "gpt-5.4"
    assert route.provider == "openai"
    assert route.credential_id > 0
    assert route.base_url == "https://healthy-a.example.com/v1"
    assert route.credential_ciphertext == "cipher-healthy-high-priority"


def test_execution_routing_repository_returns_none_when_profile_has_no_enabled_healthy_credential(db_session):
    repository = ExecutionRoutingRepositorySqlAlchemy(db_session)
    profile = ExecutionProfileRecord(
        code="claude-sonnet-4-6",
        display_name="Claude CLI / claude-sonnet-4-6",
        agent_backend="claude",
        model="claude-sonnet-4-6",
        description="备用组合",
        enabled=True,
        recommended=False,
        sort_order=20,
    )
    db_session.add(profile)
    db_session.flush()
    db_session.add(
        ServerCredentialRecord(
            execution_profile_id=profile.id,
            provider="anthropic",
            auth_type="api_key",
            credential_ciphertext="cipher-rate-limited",
            secret_ciphertext=None,
            base_url="https://api.anthropic.com",
            label="limited",
            priority=1,
            enabled=True,
            health_status="rate_limited",
            last_checked_at=None,
            last_error_code="rate_limited",
            last_error_message="limited",
        )
    )
    db_session.commit()

    route = repository.find_routable_execution_target(profile.id)

    assert route is None


def test_execution_routing_repository_can_skip_excluded_credential_ids(db_session):
    repository = ExecutionRoutingRepositorySqlAlchemy(db_session)
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
        credential_ciphertext="cipher-healthy-a",
        secret_ciphertext=None,
        base_url="https://healthy-a.example.com/v1",
        label="healthy-a",
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
        credential_ciphertext="cipher-healthy-b",
        secret_ciphertext=None,
        base_url="https://healthy-b.example.com/v1",
        label="healthy-b",
        priority=10,
        enabled=True,
        health_status="healthy",
        last_checked_at=None,
        last_error_code="",
        last_error_message="",
    )
    db_session.add_all([credential_a, credential_b])
    db_session.commit()

    route = repository.find_routable_execution_target(
        profile.id,
        excluded_credential_ids={credential_a.id},
    )

    assert route is not None
    assert route.credential_id == credential_b.id
    assert route.base_url == "https://healthy-b.example.com/v1"
