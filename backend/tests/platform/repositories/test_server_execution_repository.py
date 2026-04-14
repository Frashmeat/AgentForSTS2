from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.infra.persistence.models import ExecutionProfileRecord
from app.modules.platform.infra.persistence.repositories.server_execution_repository_sqlalchemy import (
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
