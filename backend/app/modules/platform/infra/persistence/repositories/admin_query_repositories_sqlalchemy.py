from __future__ import annotations

from sqlalchemy import or_
from sqlalchemy.orm import Session

from app.modules.auth.infra.persistence.models import UserRecord
from app.modules.platform.contracts import (
    AdminExecutionDetailView,
    AdminExecutionListItem,
    AdminExecutionProfileListItem,
    AdminQuotaLedgerItem,
    AdminQuotaLedgerListView,
    AdminServerCredentialListItem,
    AdminUserDetailView,
    AdminUserListItem,
    AdminUserListView,
    JobEventView,
    RefundRecordView,
    UserQuotaView,
)
from app.modules.platform.domain.repositories import AdminQueryRepositories
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ChargeStatus,
    ExecutionProfileRecord,
    ExecutionChargeRecord,
    JobEventRecord,
    QuotaBalanceRecord,
    UsageLedgerRecord,
    ServerCredentialRecord,
)


def _to_iso(value: object | None) -> str:
    if value is None:
        return ""
    return value.isoformat()


def _enum_value(value: object) -> str:
    return value.value if hasattr(value, "value") else str(value)


def _quota_view(balance: QuotaBalanceRecord | None) -> UserQuotaView:
    if balance is None:
        return UserQuotaView()
    remaining = max(balance.total_limit + balance.adjusted_amount - balance.used_amount + balance.refunded_amount, 0)
    return UserQuotaView(
        total_limit=balance.total_limit,
        used_amount=balance.used_amount,
        refunded_amount=balance.refunded_amount,
        adjusted_amount=balance.adjusted_amount,
        remaining=remaining,
        status=_enum_value(balance.status),
    )


def _anomaly_flags(user: UserRecord, quota: UserQuotaView) -> list[str]:
    flags: list[str] = []
    if quota.remaining <= 0:
        flags.append("quota_exhausted")
    if not user.email_verified:
        flags.append("email_unverified")
    if quota.status == "suspended":
        flags.append("quota_suspended")
    if quota.status == "closed":
        flags.append("quota_closed")
    return flags


def _ledger_reason(reason_code: str) -> str:
    parts = str(reason_code or "").split(":", 2)
    if len(parts) == 3 and parts[0] == "admin":
        return parts[2]
    return str(reason_code or "")


class AdminQueryRepositoriesSqlAlchemy(AdminQueryRepositories):
    def __init__(self, session: Session) -> None:
        self.session = session

    def list_executions(self, job_id: int | None = None) -> list[AdminExecutionListItem]:
        query = self.session.query(AIExecutionRecord)
        if job_id is not None:
            query = query.filter(AIExecutionRecord.job_id == job_id)
        rows = query.order_by(AIExecutionRecord.created_at.desc(), AIExecutionRecord.id.desc()).all()
        return [
            AdminExecutionListItem(
                id=row.id,
                job_id=row.job_id,
                job_item_id=row.job_item_id,
                status=_enum_value(row.status),
                provider=row.provider,
                model=row.model,
                credential_ref=row.credential_ref,
                retry_attempt=row.retry_attempt,
                switched_credential=row.switched_credential,
            )
            for row in rows
        ]

    def get_execution_detail(self, execution_id: int) -> AdminExecutionDetailView | None:
        row = self.session.query(AIExecutionRecord).filter(AIExecutionRecord.id == execution_id).one_or_none()
        if row is None:
            return None
        return AdminExecutionDetailView(
            id=row.id,
            job_id=row.job_id,
            job_item_id=row.job_item_id,
            status=_enum_value(row.status),
            provider=row.provider,
            model=row.model,
            credential_ref=row.credential_ref,
            retry_attempt=row.retry_attempt,
            switched_credential=row.switched_credential,
            request_idempotency_key=row.request_idempotency_key,
            input_summary=row.input_summary,
            result_summary=row.result_summary,
            error_summary=row.error_summary,
            step_protocol_version=row.step_protocol_version,
            result_schema_version=row.result_schema_version,
        )

    def list_refunds(self, user_id: int | None = None) -> list[RefundRecordView]:
        query = self.session.query(ExecutionChargeRecord).filter(ExecutionChargeRecord.charge_status == ChargeStatus.REFUNDED)
        if user_id is not None:
            query = query.filter(ExecutionChargeRecord.user_id == user_id)
        rows = query.order_by(ExecutionChargeRecord.id.asc()).all()
        return [
            RefundRecordView(
                ai_execution_id=row.ai_execution_id,
                charge_status=_enum_value(row.charge_status),
                refund_reason=row.refund_reason,
            )
            for row in rows
        ]

    def list_users(
        self,
        query: str = "",
        email_verified: bool | None = None,
        is_admin: bool | None = None,
        quota_status: str = "",
        anomaly: str = "",
        limit: int = 50,
        offset: int = 0,
    ) -> AdminUserListView:
        normalized_query = str(query or "").strip().lower()
        normalized_status = str(quota_status or "").strip()
        normalized_anomaly = str(anomaly or "").strip()
        limit = min(max(int(limit or 50), 1), 100)
        offset = max(int(offset or 0), 0)

        query_obj = self.session.query(UserRecord)
        if normalized_query:
            like = f"%{normalized_query}%"
            predicates = [UserRecord.username.ilike(like), UserRecord.email.ilike(like)]
            if normalized_query.isdigit():
                predicates.append(UserRecord.user_id == int(normalized_query))
            query_obj = query_obj.filter(
                or_(*predicates)
            )
        if email_verified is not None:
            query_obj = query_obj.filter(UserRecord.email_verified == email_verified)
        if is_admin is not None:
            query_obj = query_obj.filter(UserRecord.is_admin == is_admin)

        rows = query_obj.order_by(UserRecord.user_id.asc()).all()
        balances = {
            balance.user_id: balance
            for balance in self.session.query(QuotaBalanceRecord)
            .filter(QuotaBalanceRecord.user_id.in_([row.user_id for row in rows] or [-1]))
            .all()
        }

        items: list[AdminUserListItem] = []
        for row in rows:
            quota = _quota_view(balances.get(row.user_id))
            flags = _anomaly_flags(row, quota)
            if normalized_status and quota.status != normalized_status:
                continue
            if normalized_anomaly and normalized_anomaly not in flags:
                continue
            items.append(
                AdminUserListItem(
                    user_id=row.user_id,
                    username=row.username,
                    email=row.email,
                    email_verified=row.email_verified,
                    is_admin=row.is_admin,
                    created_at=_to_iso(row.created_at),
                    quota=quota,
                    anomaly_flags=flags,
                )
            )

        total = len(items)
        return AdminUserListView(items=items[offset : offset + limit], total=total, limit=limit, offset=offset)

    def get_user_detail(self, user_id: int) -> AdminUserDetailView | None:
        user = self.session.query(UserRecord).filter(UserRecord.user_id == user_id).one_or_none()
        if user is None:
            return None
        quota = _quota_view(self.session.query(QuotaBalanceRecord).filter(QuotaBalanceRecord.user_id == user_id).one_or_none())
        return AdminUserDetailView(
            user_id=user.user_id,
            username=user.username,
            email=user.email,
            email_verified=user.email_verified,
            is_admin=user.is_admin,
            created_at=_to_iso(user.created_at),
            email_verified_at=None if user.email_verified_at is None else _to_iso(user.email_verified_at),
            quota=quota,
            anomaly_flags=_anomaly_flags(user, quota),
        )

    def list_user_quota_ledgers(self, user_id: int, after_id: int | None = None, limit: int = 50) -> AdminQuotaLedgerListView:
        query = self.session.query(UsageLedgerRecord).filter(UsageLedgerRecord.user_id == user_id)
        if after_id is not None:
            query = query.filter(UsageLedgerRecord.id > after_id)
        rows = query.order_by(UsageLedgerRecord.id.desc()).limit(min(max(int(limit or 50), 1), 100)).all()
        return AdminQuotaLedgerListView(
            items=[
                AdminQuotaLedgerItem(
                    ledger_id=row.id,
                    ledger_type=_enum_value(row.ledger_type),
                    amount=row.amount,
                    balance_after=row.balance_after,
                    reason_code=row.reason_code,
                    reason=_ledger_reason(row.reason_code),
                    ai_execution_id=row.ai_execution_id,
                    created_at=_to_iso(row.created_at),
                )
                for row in rows
            ]
        )

    def list_audit_events(
        self,
        job_id: int | None = None,
        after_id: int | None = None,
        limit: int = 50,
        event_type_prefix: str | None = None,
    ) -> list[JobEventView]:
        query = self.session.query(JobEventRecord)
        if job_id is not None:
            query = query.filter(JobEventRecord.job_id == job_id)
        if after_id is not None:
            query = query.filter(JobEventRecord.id > after_id)
        if str(event_type_prefix or "").strip():
            query = query.filter(JobEventRecord.event_type.like(f"{str(event_type_prefix).strip()}%"))
        rows = query.order_by(JobEventRecord.id.asc()).limit(max(int(limit or 0), 1)).all()
        return [
            JobEventView(
                event_id=row.id,
                event_type=row.event_type,
                job_id=row.job_id,
                job_item_id=row.job_item_id,
                ai_execution_id=row.ai_execution_id,
                occurred_at=_to_iso(row.created_at),
                payload=dict(row.event_payload),
            )
            for row in rows
        ]

    def list_server_credentials(self, execution_profile_id: int | None = None) -> list[AdminServerCredentialListItem]:
        query = self.session.query(ServerCredentialRecord)
        if execution_profile_id is not None:
            query = query.filter(ServerCredentialRecord.execution_profile_id == execution_profile_id)
        rows = query.order_by(
            ServerCredentialRecord.execution_profile_id.asc(),
            ServerCredentialRecord.priority.asc(),
            ServerCredentialRecord.id.asc(),
        ).all()
        return [
            AdminServerCredentialListItem(
                id=row.id,
                execution_profile_id=row.execution_profile_id,
                provider=row.provider,
                auth_type=row.auth_type,
                label=row.label,
                base_url=row.base_url,
                priority=row.priority,
                enabled=row.enabled,
                health_status=row.health_status,
                last_checked_at=_to_iso(row.last_checked_at) or None,
                last_error_code=row.last_error_code,
                last_error_message=row.last_error_message,
            )
            for row in rows
        ]

    def list_execution_profiles(self) -> list[AdminExecutionProfileListItem]:
        rows = (
            self.session.query(ExecutionProfileRecord)
            .order_by(
                ExecutionProfileRecord.recommended.desc(),
                ExecutionProfileRecord.sort_order.asc(),
                ExecutionProfileRecord.id.asc(),
            )
            .all()
        )
        return [
            AdminExecutionProfileListItem(
                id=row.id,
                code=row.code,
                display_name=row.display_name,
                agent_backend=row.agent_backend,
                model=row.model,
                enabled=row.enabled,
                recommended=row.recommended,
                sort_order=row.sort_order,
            )
            for row in rows
        ]
