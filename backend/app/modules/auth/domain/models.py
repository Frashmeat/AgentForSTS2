from __future__ import annotations

from dataclasses import dataclass, replace
from datetime import datetime


@dataclass(frozen=True, slots=True)
class UserAccount:
    user_id: int
    username: str
    email: str
    password_hash: str
    email_verified: bool
    created_at: datetime
    is_admin: bool = False
    email_verified_at: datetime | None = None

    def mark_email_verified(self, verified_at: datetime) -> "UserAccount":
        return replace(
            self,
            email_verified=True,
            email_verified_at=verified_at,
        )

    def can_use_platform(self) -> bool:
        return self.email_verified


@dataclass(frozen=True, slots=True)
class EmailVerificationTicket:
    verification_id: int
    user_id: int
    purpose: str
    code: str
    email: str
    expires_at: datetime
    created_at: datetime
    consumed_at: datetime | None = None

    def is_expired(self, now: datetime) -> bool:
        return now >= self.expires_at

    def mark_consumed(self, consumed_at: datetime) -> "EmailVerificationTicket":
        return replace(self, consumed_at=consumed_at)
