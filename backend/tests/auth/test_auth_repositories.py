from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

pytest.importorskip("sqlalchemy")

from sqlalchemy import create_engine
from sqlalchemy.orm import Session

from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.auth.infra.persistence.repositories import (
    EmailVerificationRepositorySqlAlchemy,
    UserRepositorySqlAlchemy,
)
from app.shared.infra.db.base import Base


@pytest.fixture()
def session() -> Session:
    engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
    Base.metadata.create_all(engine)
    with Session(engine) as db:
        yield db


def test_user_repository_supports_create_lookup_and_save(session: Session):
    repository = UserRepositorySqlAlchemy(session)

    created = repository.create_user("Luna", "luna@example.com", "hashed::secret")
    saved = repository.save(created.mark_email_verified(datetime(2026, 4, 3, 9, 0, tzinfo=UTC)))
    loaded_by_id = repository.get_by_user_id(created.user_id)
    loaded_by_username = repository.get_by_login("luna")
    loaded_by_email = repository.get_by_login("luna@example.com")

    assert loaded_by_id is not None
    assert loaded_by_username is not None
    assert loaded_by_email is not None
    assert loaded_by_id.user_id == created.user_id
    assert loaded_by_username.email == "luna@example.com"
    assert loaded_by_email.email_verified is True
    assert saved.email_verified is True


def test_email_verification_repository_supports_ticket_lifecycle(session: Session):
    user_repository = UserRepositorySqlAlchemy(session)
    verification_repository = EmailVerificationRepositorySqlAlchemy(session)
    created = user_repository.create_user("Luna", "luna@example.com", "hashed::secret")

    ticket = verification_repository.create_ticket(
        user_id=created.user_id,
        purpose="verify_email",
        code="code-123",
        email=created.email,
        expires_at=datetime(2026, 4, 3, 10, 0, tzinfo=UTC) + timedelta(minutes=30),
    )
    loaded_before_consumed = verification_repository.get_by_code("code-123", "verify_email")
    consumed = verification_repository.save(
        ticket.mark_consumed(datetime(2026, 4, 3, 10, 15, tzinfo=UTC))
    )
    loaded = verification_repository.get_by_code("code-123", "verify_email")

    assert loaded_before_consumed is not None
    assert loaded_before_consumed.user_id == created.user_id
    assert consumed.consumed_at == datetime(2026, 4, 3, 10, 15, tzinfo=UTC)
    assert loaded is None
