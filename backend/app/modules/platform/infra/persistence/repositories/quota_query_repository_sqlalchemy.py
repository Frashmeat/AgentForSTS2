from __future__ import annotations

from datetime import datetime

from sqlalchemy.orm import Session

from app.modules.platform.contracts import UserQuotaView
from app.modules.platform.domain.repositories import QuotaQueryRepository
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBucketRecord, QuotaBucketType


class QuotaQueryRepositorySqlAlchemy(QuotaQueryRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def get_current_quota_view(self, user_id: int, now: datetime) -> UserQuotaView | None:
        account = self.session.query(QuotaAccountRecord).filter(QuotaAccountRecord.user_id == user_id).one_or_none()
        if account is None:
            return None
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
        weekly = next((bucket for bucket in buckets if bucket.bucket_type == QuotaBucketType.WEEKLY), None)
        next_reset = min((bucket.period_end for bucket in buckets), default=None)
        return UserQuotaView(
            daily_limit=0 if daily is None else daily.quota_limit,
            daily_used=0 if daily is None else daily.used_amount,
            weekly_limit=0 if weekly is None else weekly.quota_limit,
            weekly_used=0 if weekly is None else weekly.used_amount,
            refunded=sum(bucket.refunded_amount for bucket in buckets),
            next_reset_at=None if next_reset is None else next_reset.isoformat(),
        )
