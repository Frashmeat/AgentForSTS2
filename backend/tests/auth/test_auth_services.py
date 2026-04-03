from __future__ import annotations

from dataclasses import replace
from datetime import UTC, datetime

from app.modules.auth.application.services import AuthService
from app.modules.auth.domain.models import UserAccount


class FakeUserRepository:
    def __init__(self) -> None:
        self._users: list[UserAccount] = []
        self._next_id = 1000

    def create_user(self, username: str, email: str, password_hash: str) -> UserAccount:
        user = UserAccount(
            user_id=self._next_id,
            username=username,
            email=email,
            password_hash=password_hash,
            email_verified=False,
            created_at=datetime(2026, 4, 3, 8, 0, tzinfo=UTC),
        )
        self._next_id += 1
        self._users.append(user)
        return user

    def get_by_login(self, login: str) -> UserAccount | None:
        normalized = login.strip().lower()
        for user in self._users:
            if user.username.lower() == normalized or user.email.lower() == normalized:
                return user
        return None

    def get_by_user_id(self, user_id: int) -> UserAccount | None:
        for user in self._users:
            if user.user_id == user_id:
                return user
        return None

    def save(self, user: UserAccount) -> UserAccount:
        for index, current in enumerate(self._users):
            if current.user_id == user.user_id:
                self._users[index] = user
                return user
        raise AssertionError("user not found")


class FakePasswordHasher:
    def hash_password(self, plain_text: str) -> str:
        return f"hashed::{plain_text}"

    def verify_password(self, plain_text: str, password_hash: str) -> bool:
        return password_hash == f"hashed::{plain_text}"


def test_register_user_hashes_password_and_returns_unverified_user():
    service = AuthService(
        user_repository=FakeUserRepository(),
        password_hasher=FakePasswordHasher(),
    )

    user = service.register_user(
        username="luna",
        email="luna@example.com",
        password="secret-123",
    )

    assert user.user_id == 1000
    assert user.password_hash == "hashed::secret-123"
    assert user.email_verified is False
    assert user.can_use_platform() is False


def test_authenticate_user_accepts_username_or_email():
    repository = FakeUserRepository()
    service = AuthService(
        user_repository=repository,
        password_hasher=FakePasswordHasher(),
    )
    service.register_user(
        username="luna",
        email="luna@example.com",
        password="secret-123",
    )

    assert service.authenticate_user("luna", "secret-123") is not None
    assert service.authenticate_user("luna@example.com", "secret-123") is not None
    assert service.authenticate_user("luna@example.com", "bad-password") is None


def test_mark_email_verified_persists_updated_user_state():
    repository = FakeUserRepository()
    service = AuthService(
        user_repository=repository,
        password_hasher=FakePasswordHasher(),
    )
    registered = service.register_user(
        username="luna",
        email="luna@example.com",
        password="secret-123",
    )

    verified_at = datetime(2026, 4, 3, 9, 0, tzinfo=UTC)
    verified = service.mark_email_verified(registered.user_id, verified_at)

    assert verified.email_verified is True
    assert verified.email_verified_at == verified_at
    assert repository.get_by_login("luna") == replace(
        registered,
        email_verified=True,
        email_verified_at=verified_at,
    )
