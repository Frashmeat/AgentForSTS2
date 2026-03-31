from __future__ import annotations

from abc import ABC, abstractmethod
from datetime import datetime

from app.modules.platform.contracts import UserQuotaView


class QuotaQueryRepository(ABC):
    @abstractmethod
    def get_current_quota_view(self, user_id: int, now: datetime) -> UserQuotaView | None: ...
