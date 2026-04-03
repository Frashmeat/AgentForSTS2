from __future__ import annotations

from datetime import datetime

from .ports import PasswordHasher, UserRepository


class AuthService:
    def __init__(self, user_repository: UserRepository, password_hasher: PasswordHasher) -> None:
        self.user_repository = user_repository
        self.password_hasher = password_hasher

    def register_user(self, username: str, email: str, password: str):
        password_hash = self.password_hasher.hash_password(password)
        return self.user_repository.create_user(
            username=username,
            email=email,
            password_hash=password_hash,
        )

    def authenticate_user(self, login: str, password: str):
        user = self.user_repository.get_by_login(login)
        if user is None:
            return None
        if not self.password_hasher.verify_password(password, user.password_hash):
            return None
        return user

    def mark_email_verified(self, user_id: int, verified_at: datetime):
        user = self.user_repository.get_by_user_id(user_id)
        if user is None:
            raise LookupError(f"user not found: {user_id}")
        updated = user.mark_email_verified(verified_at)
        return self.user_repository.save(updated)
