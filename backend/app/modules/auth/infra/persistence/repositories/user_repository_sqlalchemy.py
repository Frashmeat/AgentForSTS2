from __future__ import annotations

from datetime import UTC, datetime

from sqlalchemy import func, or_
from sqlalchemy.orm import Session

from app.modules.auth.application.ports import UserRepository
from app.modules.auth.domain.models import UserAccount
from app.modules.auth.infra.persistence.models import UserRecord


def _as_utc_datetime(value: datetime | None) -> datetime | None:
    if value is None:
        return None
    if value.tzinfo is None:
        return value.replace(tzinfo=UTC)
    return value.astimezone(UTC)


def _to_domain(record: UserRecord) -> UserAccount:
    return UserAccount(
        user_id=record.user_id,
        username=record.username,
        email=record.email,
        password_hash=record.password_hash,
        email_verified=record.email_verified,
        created_at=_as_utc_datetime(record.created_at),
        email_verified_at=_as_utc_datetime(record.email_verified_at),
    )


class UserRepositorySqlAlchemy(UserRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create_user(self, username: str, email: str, password_hash: str) -> UserAccount:
        record = UserRecord(
            username=username.strip(),
            email=email.strip().lower(),
            password_hash=password_hash,
            email_verified=False,
        )
        self.session.add(record)
        self.session.flush()
        return _to_domain(record)

    def get_by_login(self, login: str) -> UserAccount | None:
        normalized = login.strip().lower()
        record = (
            self.session.query(UserRecord)
            .filter(
                or_(
                    func.lower(UserRecord.username) == normalized,
                    func.lower(UserRecord.email) == normalized,
                )
            )
            .one_or_none()
        )
        return _to_domain(record) if record is not None else None

    def get_by_user_id(self, user_id: int) -> UserAccount | None:
        record = self.session.query(UserRecord).filter(UserRecord.user_id == user_id).one_or_none()
        return _to_domain(record) if record is not None else None

    def save(self, user: UserAccount) -> UserAccount:
        record = self.session.query(UserRecord).filter(UserRecord.user_id == user.user_id).one_or_none()
        if record is None:
            raise LookupError(f"user not found: {user.user_id}")
        record.username = user.username
        record.email = user.email
        record.password_hash = user.password_hash
        record.email_verified = user.email_verified
        record.email_verified_at = user.email_verified_at
        self.session.flush()
        return _to_domain(record)
