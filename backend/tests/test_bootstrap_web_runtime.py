from __future__ import annotations

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker

from app.modules.auth.infra.persistence.models import UserRecord
from app.modules.auth.application import PBKDF2PasswordHasher
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBalanceRecord
from app.shared.infra.db.base import Base
from tools import bootstrap_web_runtime


def _session_factory():
    engine = create_engine("sqlite:///:memory:")
    Base.metadata.create_all(engine)
    return sessionmaker(bind=engine, autoflush=False, autocommit=False, expire_on_commit=False)


def test_ensure_default_admin_creates_login_and_quota_records(monkeypatch):
    factory = _session_factory()
    monkeypatch.setattr(bootstrap_web_runtime, "create_session_factory", lambda _cfg: factory)
    monkeypatch.setattr(bootstrap_web_runtime, "get_config", lambda: {"database": {"url": "sqlite:///:memory:"}})

    action = bootstrap_web_runtime.ensure_default_admin()

    assert action == "created"
    with factory() as session:
        user = session.query(UserRecord).filter(UserRecord.username == "admin").one()
        assert user.email == "admin@example.com"
        assert user.email_verified is True
        assert user.is_admin is True
        assert PBKDF2PasswordHasher().verify_password("admin123456", user.password_hash)
        assert session.query(QuotaAccountRecord).filter(QuotaAccountRecord.user_id == user.user_id).one()
        balance = session.query(QuotaBalanceRecord).filter(QuotaBalanceRecord.user_id == user.user_id).one()
        assert balance.total_limit == bootstrap_web_runtime.DEFAULT_ADMIN_TOTAL_LIMIT


def test_ensure_default_admin_updates_existing_admin_password_and_flags(monkeypatch):
    factory = _session_factory()
    monkeypatch.setattr(bootstrap_web_runtime, "create_session_factory", lambda _cfg: factory)
    monkeypatch.setattr(bootstrap_web_runtime, "get_config", lambda: {"database": {"url": "sqlite:///:memory:"}})

    with factory() as session:
        session.add(
            UserRecord(
                username="admin",
                email="old@example.com",
                password_hash=PBKDF2PasswordHasher().hash_password("old-password"),
                email_verified=False,
                is_admin=False,
            )
        )
        session.commit()

    action = bootstrap_web_runtime.ensure_default_admin()

    assert action == "updated"
    with factory() as session:
        user = session.query(UserRecord).filter(UserRecord.username == "admin").one()
        assert user.email == "admin@example.com"
        assert user.email_verified is True
        assert user.is_admin is True
        assert PBKDF2PasswordHasher().verify_password("admin123456", user.password_hash)
