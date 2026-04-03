import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

import pytest

pytest.importorskip("sqlalchemy")

from sqlalchemy.orm import Session

from app.shared.infra.db.session import create_engine_from_settings, create_session_factory


def test_create_engine_from_settings_uses_database_url():
    engine = create_engine_from_settings(
        {
            "url": "sqlite+pysqlite:///:memory:",
            "echo": True,
            "pool_pre_ping": True,
        }
    )

    try:
        assert str(engine.url) == "sqlite+pysqlite:///:memory:"
        assert engine.echo is True
    finally:
        engine.dispose()


def test_create_session_factory_returns_sessionmaker_bound_to_engine():
    session_factory = create_session_factory({"url": "sqlite+pysqlite:///:memory:"})
    session = session_factory()

    try:
        assert isinstance(session, Session)
        assert str(session.bind.url) == "sqlite+pysqlite:///:memory:"
    finally:
        session.close()
        session.bind.dispose()
