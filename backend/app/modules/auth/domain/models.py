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
    email_verified_at: datetime | None = None

    def mark_email_verified(self, verified_at: datetime) -> "UserAccount":
        return replace(
            self,
            email_verified=True,
            email_verified_at=verified_at,
        )

    def can_use_platform(self) -> bool:
        return self.email_verified
