from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.infra.persistence.models import (
    ChargeStatus,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBucketRecord,
    QuotaBucketType,
)
from app.modules.platform.infra.persistence.repositories.execution_charge_repository_sqlalchemy import (
    ExecutionChargeRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.quota_account_repository_sqlalchemy import (
    QuotaAccountRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.usage_ledger_repository_sqlalchemy import (
    UsageLedgerRepositorySqlAlchemy,
)


def test_quota_billing_service_reserves_and_refunds_execution_usage(db_session):
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    service = QuotaBillingService(
        execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
        quota_account_repository=quota_repository,
        usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
    )
    now = datetime.now(UTC)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )

    charge = service.reserve(user_id=1001, execution_id=77, now=now, amount=1)
    assert charge is not None
    assert charge.charge_status == ChargeStatus.RESERVED

    refunded = service.refund(execution_id=77, now=now, reason="system_error")
    db_session.commit()

    assert refunded is not None
    assert refunded.charge_status == ChargeStatus.REFUNDED


def test_quota_billing_service_does_not_double_refund_same_execution(db_session):
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    service = QuotaBillingService(
        execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
        quota_account_repository=quota_repository,
        usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
    )
    now = datetime.now(UTC)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    bucket = quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )

    charge = service.reserve(user_id=1001, execution_id=77, now=now, amount=1)
    assert charge is not None

    first_refund = service.refund(execution_id=77, now=now, reason="execution_deferred")
    second_refund = service.refund(execution_id=77, now=now, reason="execution_deferred")
    db_session.refresh(bucket)

    assert first_refund is not None
    assert second_refund is not None
    assert bucket.refunded_amount == 1


def test_quota_billing_service_reports_quota_exhausted_when_no_available_bucket(db_session):
    service = QuotaBillingService(
        execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
        quota_account_repository=QuotaAccountRepositorySqlAlchemy(db_session),
        usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
    )

    assert service.reserve(user_id=9999, execution_id=88, now=datetime.now(UTC), amount=1) is None
