from __future__ import annotations

import argparse
import sys
from datetime import UTC, datetime
from pathlib import Path

BACKEND_ROOT = Path(__file__).resolve().parents[1]
if str(BACKEND_ROOT) not in sys.path:
    sys.path.insert(0, str(BACKEND_ROOT))

from app.modules.auth.application import PBKDF2PasswordHasher
from app.modules.auth.infra.persistence.models import UserRecord
from app.modules.platform.infra.persistence.models import (
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBalanceRecord,
)
from app.shared.infra.config.settings import get_config
from app.shared.infra.db.session import create_session_factory
from app.shared.infra.db.session_scope import session_scope

DEFAULT_ADMIN_USERNAME = "admin"
DEFAULT_ADMIN_EMAIL = "admin@example.com"
DEFAULT_ADMIN_PASSWORD = "admin123456"
DEFAULT_ADMIN_TOTAL_LIMIT = 100


def ensure_default_admin() -> str:
    session_factory = create_session_factory(get_config().get("database", {}))
    with session_scope(session_factory) as session:
        now = datetime.now(UTC)
        user = session.query(UserRecord).filter(UserRecord.username == DEFAULT_ADMIN_USERNAME).one_or_none()

        if user is None:
            user = UserRecord(
                username=DEFAULT_ADMIN_USERNAME,
                email=DEFAULT_ADMIN_EMAIL,
                created_at=now,
            )
            session.add(user)
            action = "created"
        else:
            action = "updated"

        user.email = DEFAULT_ADMIN_EMAIL
        user.password_hash = PBKDF2PasswordHasher().hash_password(DEFAULT_ADMIN_PASSWORD)
        user.email_verified = True
        user.email_verified_at = now
        user.is_admin = True
        user.deleted_at = None
        user.updated_at = now
        session.flush()

        quota_account = session.query(QuotaAccountRecord).filter(QuotaAccountRecord.user_id == user.user_id).one_or_none()
        if quota_account is None:
            quota_account = QuotaAccountRecord(
                user_id=user.user_id,
                status=QuotaAccountStatus.ACTIVE,
                created_at=now,
                updated_at=now,
            )
            session.add(quota_account)
            session.flush()
        else:
            quota_account.status = QuotaAccountStatus.ACTIVE
            quota_account.updated_at = now

        quota_balance = (
            session.query(QuotaBalanceRecord).filter(QuotaBalanceRecord.user_id == user.user_id).one_or_none()
        )
        if quota_balance is None:
            session.add(
                QuotaBalanceRecord(
                    user_id=user.user_id,
                    quota_account_id=quota_account.id,
                    total_limit=DEFAULT_ADMIN_TOTAL_LIMIT,
                    status=QuotaAccountStatus.ACTIVE,
                    created_at=now,
                    updated_at=now,
                )
            )
        else:
            quota_balance.quota_account_id = quota_account.id
            quota_balance.status = QuotaAccountStatus.ACTIVE
            quota_balance.updated_at = now

        return action


def main() -> int:
    parser = argparse.ArgumentParser(description="Bootstrap web runtime data.")
    parser.add_argument("--ensure-default-admin", action="store_true", help="Create the default admin account if missing.")
    args = parser.parse_args()

    if args.ensure_default_admin:
        action = ensure_default_admin()
        if action == "created":
            print(
                "已创建默认管理员账号: "
                f"{DEFAULT_ADMIN_USERNAME} / {DEFAULT_ADMIN_EMAIL} / {DEFAULT_ADMIN_PASSWORD}"
            )
        else:
            print(
                "已更新默认管理员账号: "
                f"{DEFAULT_ADMIN_USERNAME} / {DEFAULT_ADMIN_EMAIL} / {DEFAULT_ADMIN_PASSWORD}"
            )
        return 0

    parser.print_help()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
