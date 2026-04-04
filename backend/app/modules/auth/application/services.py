from __future__ import annotations

import hashlib
import hmac
import os
from datetime import datetime

from .ports import PasswordHasher, UserRepository


class PBKDF2PasswordHasher:
    def __init__(self, iterations: int = 600_000) -> None:
        self.iterations = iterations

    def hash_password(self, plain_text: str) -> str:
        salt = os.urandom(16)
        derived = hashlib.pbkdf2_hmac("sha256", plain_text.encode("utf-8"), salt, self.iterations)
        return f"pbkdf2_sha256${self.iterations}${salt.hex()}${derived.hex()}"

    def verify_password(self, plain_text: str, password_hash: str) -> bool:
        try:
            algorithm, iterations, salt_hex, digest_hex = password_hash.split("$", 3)
        except ValueError:
            return False
        if algorithm != "pbkdf2_sha256":
            return False
        derived = hashlib.pbkdf2_hmac(
            "sha256",
            plain_text.encode("utf-8"),
            bytes.fromhex(salt_hex),
            int(iterations),
        )
        return hmac.compare_digest(derived.hex(), digest_hex)


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

    def get_user_by_id(self, user_id: int):
        return self.user_repository.get_by_user_id(user_id)

    def reset_password(self, user_id: int, password: str):
        user = self.user_repository.get_by_user_id(user_id)
        if user is None:
            raise LookupError(f"user not found: {user_id}")
        updated = type(user)(
            user_id=user.user_id,
            username=user.username,
            email=user.email,
            password_hash=self.password_hasher.hash_password(password),
            email_verified=user.email_verified,
            created_at=user.created_at,
            email_verified_at=user.email_verified_at,
        )
        return self.user_repository.save(updated)
