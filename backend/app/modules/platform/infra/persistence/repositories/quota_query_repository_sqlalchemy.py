from __future__ import annotations

from datetime import datetime

from sqlalchemy.orm import Session

from app.modules.platform.contracts import UserQuotaView
from app.modules.platform.domain.repositories import QuotaQueryRepository
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBalanceRecord, QuotaBucketRecord, QuotaBucketType


def _remaining(total_limit: int, used_amount: int, refunded_amount: int, adjusted_amount: int) -> int:
    return max(total_limit + adjusted_amount - used_amount + refunded_amount, 0)


class QuotaQueryRepositorySqlAlchemy(QuotaQueryRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def get_current_quota_view(self, user_id: int, now: datetime) -> UserQuotaView | None:
        account = self.session.query(QuotaAccountRecord).filter(QuotaAccountRecord.user_id == user_id).one_or_none()
        if account is None:
            return UserQuotaView()
        balance = self.session.query(QuotaBalanceRecord).filter(QuotaBalanceRecord.user_id == user_id).one_or_none()
        if balance is not None:
            return UserQuotaView(
                total_limit=balance.total_limit,
                used_amount=balance.used_amount,
                refunded_amount=balance.refunded_amount,
                adjusted_amount=balance.adjusted_amount,
                remaining=_remaining(
                    balance.total_limit,
                    balance.used_amount,
                    balance.refunded_amount,
                    balance.adjusted_amount,
                ),
                status=balance.status.value if hasattr(balance.status, "value") else str(balance.status),
            )
        buckets = (
            self.session.query(QuotaBucketRecord)
            .filter(
                QuotaBucketRecord.quota_account_id == account.id,
                QuotaBucketRecord.period_start <= now,
                QuotaBucketRecord.period_end > now,
            )
            .all()
        )
        daily = next((bucket for bucket in buckets if bucket.bucket_type == QuotaBucketType.DAILY), None)
        legacy = daily or next((bucket for bucket in buckets if bucket.bucket_type == QuotaBucketType.WEEKLY), None)
        if legacy is None:
            return UserQuotaView(status=account.status.value if hasattr(account.status, "value") else str(account.status))
        return UserQuotaView(
            total_limit=legacy.quota_limit,
            used_amount=legacy.used_amount,
            refunded_amount=legacy.refunded_amount,
            adjusted_amount=0,
            remaining=_remaining(legacy.quota_limit, legacy.used_amount, legacy.refunded_amount, 0),
            status=account.status.value if hasattr(account.status, "value") else str(account.status),
        )
