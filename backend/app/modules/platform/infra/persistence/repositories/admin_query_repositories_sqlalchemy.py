from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.contracts import AdminExecutionDetailView, AdminExecutionListItem, JobEventView, RefundRecordView
from app.modules.platform.domain.repositories import AdminQueryRepositories
from app.modules.platform.infra.persistence.models import AIExecutionRecord, ChargeStatus, ExecutionChargeRecord, JobEventRecord


def _to_iso(value: object | None) -> str:
    if value is None:
        return ""
    return value.isoformat()


def _enum_value(value: object) -> str:
    return value.value if hasattr(value, "value") else str(value)


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

    def list_audit_events(self, job_id: int | None = None) -> list[JobEventView]:
        query = self.session.query(JobEventRecord)
        if job_id is not None:
            query = query.filter(JobEventRecord.job_id == job_id)
        rows = query.order_by(JobEventRecord.id.asc()).all()
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
