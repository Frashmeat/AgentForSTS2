from __future__ import annotations

from datetime import datetime

from app.modules.platform.domain.repositories import (
    ExecutionChargeRepository,
    QuotaAccountRepository,
    UsageLedgerRepository,
)
from app.modules.platform.infra.persistence.models import ChargeStatus, ExecutionChargeRecord, LedgerType, UsageLedgerRecord


class QuotaBillingService:
    def __init__(
        self,
        execution_charge_repository: ExecutionChargeRepository,
        quota_account_repository: QuotaAccountRepository,
        usage_ledger_repository: UsageLedgerRepository,
    ) -> None:
        self.execution_charge_repository = execution_charge_repository
        self.quota_account_repository = quota_account_repository
        self.usage_ledger_repository = usage_ledger_repository

    def has_available_quota(self, user_id: int, now: datetime, amount: int = 1) -> bool:
        account = self.quota_account_repository.find_account_by_user_id_for_update(user_id)
        if account is None:
            return False
        bucket = self.quota_account_repository.find_active_bucket_for_update(account.id, now)
        if bucket is None:
            return False
        return bucket.used_amount - bucket.refunded_amount + amount <= bucket.quota_limit

    def reserve(self, user_id: int, execution_id: int, now: datetime, amount: int = 1) -> ExecutionChargeRecord | None:
        account = self.quota_account_repository.find_account_by_user_id_for_update(user_id)
        if account is None:
            return None
        bucket = self.quota_account_repository.find_active_bucket_for_update(account.id, now)
        if bucket is None or bucket.used_amount - bucket.refunded_amount + amount > bucket.quota_limit:
            return None

        bucket.used_amount += amount
        self.quota_account_repository.save_bucket(bucket)

        charge = self.execution_charge_repository.create_reserved(
            ExecutionChargeRecord(
                ai_execution_id=execution_id,
                user_id=user_id,
                charge_status=ChargeStatus.RESERVED,
                charge_amount=amount,
                reserved_at=now,
            )
        )
        self.usage_ledger_repository.append_reserve(
            UsageLedgerRecord(
                user_id=user_id,
                quota_account_id=account.id,
                quota_bucket_id=bucket.id,
                ai_execution_id=execution_id,
                ledger_type=LedgerType.RESERVE,
                amount=amount,
                balance_after=max(bucket.quota_limit - bucket.used_amount + bucket.refunded_amount, 0),
                reason_code="execution_start",
            )
        )
        return charge

    def capture(self, execution_id: int, now: datetime, reason_code: str = "execution_finish") -> ExecutionChargeRecord | None:
        charge = self.execution_charge_repository.find_by_execution_id_for_update(execution_id)
        if charge is None:
            return None
        charge.charge_status = ChargeStatus.CAPTURED
        charge.captured_at = now
        self.execution_charge_repository.save(charge)

        ledgers = self.usage_ledger_repository.list_by_execution_id(execution_id)
        if ledgers:
            last = ledgers[-1]
            self.usage_ledger_repository.append_capture(
                UsageLedgerRecord(
                    user_id=charge.user_id,
                    quota_account_id=last.quota_account_id,
                    quota_bucket_id=last.quota_bucket_id,
                    ai_execution_id=execution_id,
                    ledger_type=LedgerType.CAPTURE,
                    amount=charge.charge_amount,
                    balance_after=last.balance_after,
                    reason_code=reason_code,
                )
            )
        return charge

    def refund(self, execution_id: int, now: datetime, reason: str) -> ExecutionChargeRecord | None:
        charge = self.execution_charge_repository.find_by_execution_id_for_update(execution_id)
        if charge is None:
            return None
        if charge.charge_status == ChargeStatus.REFUNDED:
            return charge
        ledgers = self.usage_ledger_repository.list_by_execution_id(execution_id)
        reserve_entry = ledgers[0] if ledgers else None
        if reserve_entry is not None:
            bucket = self.quota_account_repository.find_active_bucket_for_update(reserve_entry.quota_account_id, now)
            if bucket is not None:
                bucket.refunded_amount += charge.charge_amount
                self.quota_account_repository.save_bucket(bucket)
                self.usage_ledger_repository.append_refund(
                    UsageLedgerRecord(
                        user_id=charge.user_id,
                        quota_account_id=reserve_entry.quota_account_id,
                        quota_bucket_id=reserve_entry.quota_bucket_id,
                        ai_execution_id=execution_id,
                        ledger_type=LedgerType.REFUND,
                        amount=charge.charge_amount,
                        balance_after=bucket.quota_limit - bucket.used_amount + bucket.refunded_amount,
                        reason_code=reason,
                    )
                )
        charge.charge_status = ChargeStatus.REFUNDED
        charge.refund_reason = reason
        charge.refunded_at = now
        self.execution_charge_repository.save(charge)
        return charge
