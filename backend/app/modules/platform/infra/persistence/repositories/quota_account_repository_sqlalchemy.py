from __future__ import annotations

from datetime import datetime

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import QuotaAccountRepository
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBucketRecord


class QuotaAccountRepositorySqlAlchemy(QuotaAccountRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create_account(self, account: QuotaAccountRecord) -> QuotaAccountRecord:
        self.session.add(account)
        self.session.flush()
        return account

    def create_bucket(self, bucket: QuotaBucketRecord) -> QuotaBucketRecord:
        self.session.add(bucket)
        self.session.flush()
        return bucket

    def find_account_by_user_id(self, user_id: int) -> QuotaAccountRecord | None:
        return self.session.query(QuotaAccountRecord).filter(QuotaAccountRecord.user_id == user_id).one_or_none()

    def find_account_by_user_id_for_update(self, user_id: int) -> QuotaAccountRecord | None:
        return self.find_account_by_user_id(user_id)

    def find_active_bucket_for_update(self, quota_account_id: int, now: datetime) -> QuotaBucketRecord | None:
        return (
            self.session.query(QuotaBucketRecord)
            .filter(
                QuotaBucketRecord.quota_account_id == quota_account_id,
                QuotaBucketRecord.period_start <= now,
                QuotaBucketRecord.period_end > now,
            )
            .order_by(QuotaBucketRecord.period_start.desc(), QuotaBucketRecord.id.desc())
            .first()
        )

    def save_account(self, account: QuotaAccountRecord) -> None:
        self.session.add(account)
        self.session.flush()

    def save_bucket(self, bucket: QuotaBucketRecord) -> None:
        self.session.add(bucket)
        self.session.flush()
