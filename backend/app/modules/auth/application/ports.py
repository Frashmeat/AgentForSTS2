from __future__ import annotations

from datetime import datetime
from typing import Protocol

from app.modules.auth.domain.models import UserAccount


class UserRepository(Protocol):
    def create_user(self, username: str, email: str, password_hash: str) -> UserAccount: ...

    def get_by_login(self, login: str) -> UserAccount | None: ...

    def get_by_user_id(self, user_id: int) -> UserAccount | None: ...

    def save(self, user: UserAccount) -> UserAccount: ...


class PasswordHasher(Protocol):
    def hash_password(self, plain_text: str) -> str: ...

    def verify_password(self, plain_text: str, password_hash: str) -> bool: ...
