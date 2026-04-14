from __future__ import annotations

from datetime import UTC, datetime

from app.modules.auth.domain.models import UserAccount


def test_user_account_starts_unverified_and_cannot_use_platform():
    user = UserAccount(
        user_id=101,
        username="luna",
        email="luna@example.com",
        password_hash="hashed::secret",
        email_verified=False,
        created_at=datetime(2026, 4, 3, 8, 0, tzinfo=UTC),
    )

    assert user.user_id == 101
    assert user.email_verified is False
    assert user.is_admin is False
    assert user.can_use_platform() is False


def test_user_account_mark_email_verified_enables_platform_usage():
    user = UserAccount(
        user_id=101,
        username="luna",
        email="luna@example.com",
        password_hash="hashed::secret",
        email_verified=False,
        created_at=datetime(2026, 4, 3, 8, 0, tzinfo=UTC),
    )

    verified_at = datetime(2026, 4, 3, 8, 30, tzinfo=UTC)
    updated = user.mark_email_verified(verified_at)

    assert updated.email_verified is True
    assert updated.email_verified_at == verified_at
    assert updated.is_admin is False
    assert updated.can_use_platform() is True
