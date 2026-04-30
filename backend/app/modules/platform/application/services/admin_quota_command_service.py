from __future__ import annotations

from datetime import datetime

from app.modules.platform.contracts import AdjustUserQuotaCommand, UserQuotaView
from app.modules.platform.domain.repositories import (
    QuotaAccountRepository,
    QuotaBalanceRepository,
    UsageLedgerRepository,
)
from app.modules.platform.infra.persistence.models import (
    LedgerType,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBalanceRecord,
    UsageLedgerRecord,
)


class AdminQuotaCommandService:
    def __init__(
        self,
        quota_account_repository: QuotaAccountRepository,
        quota_balance_repository: QuotaBalanceRepository,
        usage_ledger_repository: UsageLedgerRepository,
    ) -> None:
        self.quota_account_repository = quota_account_repository
        self.quota_balance_repository = quota_balance_repository
        self.usage_ledger_repository = usage_ledger_repository

    @staticmethod
    def _remaining(balance: QuotaBalanceRecord) -> int:
        return max(balance.total_limit + balance.adjusted_amount - balance.used_amount + balance.refunded_amount, 0)

    @classmethod
    def _view(cls, balance: QuotaBalanceRecord) -> UserQuotaView:
        return UserQuotaView(
            total_limit=balance.total_limit,
            used_amount=balance.used_amount,
            refunded_amount=balance.refunded_amount,
            adjusted_amount=balance.adjusted_amount,
            remaining=cls._remaining(balance),
            status=balance.status.value if hasattr(balance.status, "value") else str(balance.status),
        )

    def _ensure_balance(self, user_id: int) -> QuotaBalanceRecord:
        balance = self.quota_balance_repository.find_by_user_id_for_update(user_id)
        if balance is not None:
            return balance

        account = self.quota_account_repository.find_account_by_user_id_for_update(user_id)
        if account is None:
            account = self.quota_account_repository.create_account(
                QuotaAccountRecord(user_id=user_id, status=QuotaAccountStatus.ACTIVE)
            )

        return self.quota_balance_repository.create_balance(
            QuotaBalanceRecord(
                user_id=user_id,
                quota_account_id=account.id,
                total_limit=0,
                used_amount=0,
                refunded_amount=0,
                adjusted_amount=0,
                status=account.status,
            )
        )

    def adjust_user_quota(
        self,
        user_id: int,
        command: AdjustUserQuotaCommand,
        *,
        admin_user_id: int,
        now: datetime,
    ) -> UserQuotaView:
        direction = str(command.direction or "").strip()
        reason = str(command.reason or "").strip()
        amount = int(command.amount)
        if direction not in {"grant", "deduct"}:
            raise ValueError("direction must be grant or deduct")
        if amount <= 0:
            raise ValueError("amount must be positive")
        if not reason:
            raise ValueError("reason is required")

        balance = self._ensure_balance(user_id)
        if direction == "deduct" and self._remaining(balance) < amount:
            raise ValueError("quota adjustment cannot make remaining quota negative")

        signed_amount = amount if direction == "grant" else -amount
        balance.adjusted_amount += signed_amount
        self.quota_balance_repository.save_balance(balance)

        ledger_type = LedgerType.ADMIN_GRANT if direction == "grant" else LedgerType.ADMIN_DEDUCT
        self.usage_ledger_repository.append_admin_adjustment(
            UsageLedgerRecord(
                user_id=user_id,
                quota_account_id=balance.quota_account_id,
                quota_balance_id=balance.id,
                quota_bucket_id=None,
                ai_execution_id=None,
                ledger_type=ledger_type,
                amount=amount,
                balance_after=self._remaining(balance),
                reason_code=f"admin:{admin_user_id}:{reason}",
            )
        )
        return self._view(balance)
