from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

import pytest

from app.modules.platform.contracts import CreateExecutionProfileCommand, UpdateExecutionProfileCommand
from app.modules.platform.infra.persistence.models import (
    ExecutionProfileRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)
from app.modules.platform.infra.persistence.repositories.server_execution_repository_sqlalchemy import (
    ExecutionProfileInUseError,
    ServerExecutionRepositorySqlAlchemy,
)


def test_server_execution_repository_seeds_default_execution_profiles_once(db_session):
    repository = ServerExecutionRepositorySqlAlchemy(db_session)

    repository.ensure_default_execution_profiles_seeded()
    repository.ensure_default_execution_profiles_seeded()
    db_session.commit()

    rows = (
        db_session.query(ExecutionProfileRecord)
        .order_by(ExecutionProfileRecord.sort_order.asc(), ExecutionProfileRecord.id.asc())
        .all()
    )

    assert [row.code for row in rows] == [
        "codex-gpt-5-4",
        "claude-cli-claude-sonnet-4-6",
    ]


def test_server_execution_repository_manages_execution_profiles(db_session):
    repository = ServerExecutionRepositorySqlAlchemy(db_session)

    created = repository.create_execution_profile(
        CreateExecutionProfileCommand.model_validate(
            {
                "code": "codex-gpt-5-5",
                "display_name": "Codex CLI / gpt-5.5",
                "agent_backend": "codex",
                "model": "gpt-5.5",
                "description": "新模型配置",
                "enabled": True,
                "recommended": False,
                "sort_order": 30,
            }
        )
    )

    assert created.code == "codex-gpt-5-5"
    assert created.model == "gpt-5.5"

    with pytest.raises(ValueError, match="execution profile code already exists"):
        repository.create_execution_profile(
            CreateExecutionProfileCommand.model_validate(
                {
                    "code": "codex-gpt-5-5",
                    "display_name": "Duplicate",
                    "agent_backend": "codex",
                    "model": "gpt-5.5",
                }
            )
        )

    updated = repository.update_execution_profile(
        created.id,
        UpdateExecutionProfileCommand.model_validate(
            {
                "code": "codex-gpt-5-5-latest",
                "display_name": "Codex CLI / gpt-5.5 latest",
                "agent_backend": "codex",
                "model": "gpt-5.5",
                "description": "更新后的配置",
                "enabled": True,
                "recommended": True,
                "sort_order": 5,
            }
        ),
    )

    assert updated.code == "codex-gpt-5-5-latest"
    assert updated.recommended is True

    disabled = repository.set_execution_profile_enabled(created.id, False)
    assert disabled.enabled is False

    enabled = repository.set_execution_profile_enabled(created.id, True)
    assert enabled.enabled is True

    assert repository.delete_execution_profile(created.id) is True
    assert db_session.query(ExecutionProfileRecord).filter_by(id=created.id).one_or_none() is None


def test_server_execution_repository_rejects_deleting_referenced_execution_profile(db_session):
    repository = ServerExecutionRepositorySqlAlchemy(db_session)
    profile = ExecutionProfileRecord(
        code="codex-referenced",
        display_name="Codex referenced",
        agent_backend="codex",
        model="gpt-5.4",
        description="",
        enabled=True,
        recommended=False,
        sort_order=10,
    )
    db_session.add(profile)
    db_session.flush()
    db_session.add(
        ServerCredentialRecord(
            execution_profile_id=profile.id,
            provider="openai",
            auth_type="api_key",
            credential_ciphertext="cipher",
            secret_ciphertext=None,
            base_url="",
            label="referenced",
            priority=1,
            enabled=True,
            health_status="healthy",
        )
    )
    db_session.flush()

    with pytest.raises(ExecutionProfileInUseError, match="execution profile is referenced"):
        repository.delete_execution_profile(profile.id)

    preferred = ExecutionProfileRecord(
        code="codex-preferred",
        display_name="Codex preferred",
        agent_backend="codex",
        model="gpt-5.4",
        description="",
        enabled=True,
        recommended=False,
        sort_order=20,
    )
    db_session.add(preferred)
    db_session.flush()
    db_session.add(UserPlatformPreferenceRecord(user_id=1001, default_execution_profile_id=preferred.id))
    db_session.flush()

    with pytest.raises(ExecutionProfileInUseError, match="execution profile is referenced"):
        repository.delete_execution_profile(preferred.id)
