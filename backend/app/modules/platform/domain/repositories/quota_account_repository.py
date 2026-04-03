from __future__ import annotations

from abc import ABC, abstractmethod
from datetime import datetime

from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaBucketRecord


class QuotaAccountRepository(ABC):
    @abstractmethod
    def create_account(self, account: QuotaAccountRecord) -> QuotaAccountRecord: ...

    @abstractmethod
    def create_bucket(self, bucket: QuotaBucketRecord) -> QuotaBucketRecord: ...

    @abstractmethod
    def find_account_by_user_id(self, user_id: int) -> QuotaAccountRecord | None: ...

    @abstractmethod
    def find_account_by_user_id_for_update(self, user_id: int) -> QuotaAccountRecord | None: ...

    @abstractmethod
    def find_active_bucket_for_update(self, quota_account_id: int, now: datetime) -> QuotaBucketRecord | None: ...

    @abstractmethod
    def save_account(self, account: QuotaAccountRecord) -> None: ...

    @abstractmethod
    def save_bucket(self, bucket: QuotaBucketRecord) -> None: ...
