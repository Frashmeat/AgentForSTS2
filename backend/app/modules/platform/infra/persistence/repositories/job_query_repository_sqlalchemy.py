from __future__ import annotations

from dataclasses import dataclass, field

from app.modules.platform.contracts import ArtifactSummary, JobDetailView, JobEventView, JobItemListItem, JobListItem
from app.modules.platform.domain.repositories import JobQueryRepository
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ArtifactRecord,
    ChargeStatus,
    ExecutionChargeRecord,
    JobEventRecord,
    JobItemRecord,
    JobRecord,
)
from sqlalchemy.orm import Session


def _to_iso(value: object | None) -> str:
    if value is None:
        return ""
    return value.isoformat()


def _enum_value(value: object) -> str:
    return value.value if hasattr(value, "value") else str(value)


@dataclass(slots=True)
class _RefundSummary:
    original_deducted: int = 0
    refunded_amount: int = 0
    refund_reasons: list[str] = field(default_factory=list)

    @property
    def net_consumed(self) -> int:
        return max(self.original_deducted - self.refunded_amount, 0)

    @property
    def refund_reason_summary(self) -> str:
        return ", ".join(self.refund_reasons)

    def append_reason(self, reason: str) -> None:
        normalized = reason.strip()
        if not normalized or normalized in self.refund_reasons:
            return
        self.refund_reasons.append(normalized)


@dataclass(slots=True)
class _DeferredSummary:
    reason_code: str = ""
    reason_message: str = ""


@dataclass(slots=True)
class _QueuedSummary:
    reason_code: str = ""
    reason_message: str = ""


@dataclass(slots=True)
class _DeliverySummary:
    state: str = ""


class JobQueryRepositorySqlAlchemy(JobQueryRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def list_jobs(self, user_id: int) -> list[JobListItem]:
        rows = (
            self.session.query(JobRecord)
            .filter(JobRecord.user_id == user_id)
            .order_by(JobRecord.created_at.desc(), JobRecord.id.desc())
            .all()
        )
        refund_summaries = self._load_refund_summaries([row.id for row in rows])
        queued_summaries = self._load_queued_summaries([row.id for row in rows])
        deferred_summaries = self._load_deferred_summaries([row.id for row in rows])
        delivery_summaries = self._load_delivery_summaries([row.id for row in rows])
        return [
            JobListItem(
                id=row.id,
                job_type=row.job_type,
                status=_enum_value(row.status),
                delivery_state=delivery_summaries.get(row.id, _DeliverySummary()).state,
                input_summary=row.input_summary,
                selected_execution_profile_id=row.selected_execution_profile_id,
                selected_agent_backend=row.selected_agent_backend,
                selected_model=row.selected_model,
                result_summary=row.result_summary,
                total_item_count=row.total_item_count,
                succeeded_item_count=row.succeeded_item_count,
                failed_item_count=row.failed_business_item_count + row.failed_system_item_count,
                original_deducted=refund_summaries.get(row.id, _RefundSummary()).original_deducted,
                refunded_amount=refund_summaries.get(row.id, _RefundSummary()).refunded_amount,
                net_consumed=refund_summaries.get(row.id, _RefundSummary()).net_consumed,
                refund_reason_summary=refund_summaries.get(row.id, _RefundSummary()).refund_reason_summary,
                queued_reason_code=(
                    queued_summaries.get(row.id, _QueuedSummary()).reason_code if _enum_value(row.status) == "queued" else ""
                ),
                queued_reason_message=(
                    queued_summaries.get(row.id, _QueuedSummary()).reason_message if _enum_value(row.status) == "queued" else ""
                ),
                deferred_reason_code=deferred_summaries.get(row.id, _DeferredSummary()).reason_code,
                deferred_reason_message=deferred_summaries.get(row.id, _DeferredSummary()).reason_message,
            )
            for row in rows
        ]

    def get_job_detail(self, user_id: int, job_id: int) -> JobDetailView | None:
        row = self.session.query(JobRecord).filter(JobRecord.id == job_id, JobRecord.user_id == user_id).one_or_none()
        if row is None:
            return None
        refund_summary = self._load_refund_summaries([row.id]).get(row.id, _RefundSummary())
        queued_summary = self._load_queued_summaries([row.id]).get(row.id, _QueuedSummary())
        deferred_summary = self._load_deferred_summaries([row.id]).get(row.id, _DeferredSummary())
        delivery_summary = self._load_delivery_summaries([row.id]).get(row.id, _DeliverySummary())
        return JobDetailView(
            id=row.id,
            job_type=row.job_type,
            status=_enum_value(row.status),
            delivery_state=delivery_summary.state,
            input_summary=row.input_summary,
            selected_execution_profile_id=row.selected_execution_profile_id,
            selected_agent_backend=row.selected_agent_backend,
            selected_model=row.selected_model,
            result_summary=row.result_summary,
            error_summary=row.error_summary,
            original_deducted=refund_summary.original_deducted,
            refunded_amount=refund_summary.refunded_amount,
            net_consumed=refund_summary.net_consumed,
            refund_reason_summary=refund_summary.refund_reason_summary,
            queued_reason_code=queued_summary.reason_code if _enum_value(row.status) == "queued" else "",
            queued_reason_message=queued_summary.reason_message if _enum_value(row.status) == "queued" else "",
            deferred_reason_code=deferred_summary.reason_code,
            deferred_reason_message=deferred_summary.reason_message,
            items=self.list_job_items(user_id, job_id),
            artifacts=self.list_artifact_summaries(user_id, job_id),
        )

    def list_job_items(self, user_id: int, job_id: int) -> list[JobItemListItem]:
        rows = (
            self.session.query(JobItemRecord)
            .join(JobRecord, JobRecord.id == JobItemRecord.job_id)
            .filter(JobRecord.user_id == user_id, JobItemRecord.job_id == job_id)
            .order_by(JobItemRecord.item_index.asc())
            .all()
        )
        delivery_summaries = self._load_item_delivery_summaries([row.id for row in rows])
        return [
            JobItemListItem(
                id=row.id,
                item_index=row.item_index,
                item_type=row.item_type,
                status=_enum_value(row.status),
                delivery_state=delivery_summaries.get(row.id, _DeliverySummary()).state,
                result_summary=row.result_summary,
                error_summary=row.error_summary,
            )
            for row in rows
        ]

    def _load_refund_summaries(self, job_ids: list[int]) -> dict[int, _RefundSummary]:
        if not job_ids:
            return {}

        rows = (
            self.session.query(
                AIExecutionRecord.job_id,
                ExecutionChargeRecord.charge_amount,
                ExecutionChargeRecord.charge_status,
                ExecutionChargeRecord.refund_reason,
            )
            .join(ExecutionChargeRecord, ExecutionChargeRecord.ai_execution_id == AIExecutionRecord.id)
            .filter(AIExecutionRecord.job_id.in_(job_ids))
            .all()
        )

        summaries: dict[int, _RefundSummary] = {}
        for job_id, charge_amount, charge_status, refund_reason in rows:
            summary = summaries.setdefault(job_id, _RefundSummary())
            summary.original_deducted += int(charge_amount or 0)
            if charge_status == ChargeStatus.REFUNDED:
                summary.refunded_amount += int(charge_amount or 0)
                summary.append_reason(str(refund_reason or ""))
        return summaries

    def _load_deferred_summaries(self, job_ids: list[int]) -> dict[int, _DeferredSummary]:
        if not job_ids:
            return {}

        rows = (
            self.session.query(JobEventRecord)
            .filter(JobEventRecord.job_id.in_(job_ids), JobEventRecord.event_type == "ai_execution.deferred")
            .order_by(JobEventRecord.job_id.asc(), JobEventRecord.id.desc())
            .all()
        )

        summaries: dict[int, _DeferredSummary] = {}
        for row in rows:
            if row.job_id in summaries:
                continue
            payload = dict(row.event_payload or {})
            summaries[row.job_id] = _DeferredSummary(
                reason_code=str(payload.get("reason_code", "")).strip(),
                reason_message=str(payload.get("reason_message", "")).strip(),
            )
        return summaries

    def _load_queued_summaries(self, job_ids: list[int]) -> dict[int, _QueuedSummary]:
        if not job_ids:
            return {}

        rows = (
            self.session.query(JobEventRecord)
            .filter(JobEventRecord.job_id.in_(job_ids), JobEventRecord.event_type == "job.queued")
            .order_by(JobEventRecord.job_id.asc(), JobEventRecord.id.desc())
            .all()
        )

        summaries: dict[int, _QueuedSummary] = {}
        for row in rows:
            if row.job_id in summaries:
                continue
            payload = dict(row.event_payload or {})
            summaries[row.job_id] = _QueuedSummary(
                reason_code=str(payload.get("reason_code", "")).strip(),
                reason_message=str(payload.get("reason_message", "")).strip(),
            )
        return summaries

    def _load_delivery_summaries(self, job_ids: list[int]) -> dict[int, _DeliverySummary]:
        if not job_ids:
            return {}

        rows = (
            self.session.query(ArtifactRecord.job_id, ArtifactRecord.artifact_type)
            .filter(ArtifactRecord.job_id.in_(job_ids))
            .order_by(ArtifactRecord.job_id.asc(), ArtifactRecord.id.asc())
            .all()
        )

        summaries: dict[int, _DeliverySummary] = {}
        for job_id, artifact_type in rows:
            summary = summaries.setdefault(job_id, _DeliverySummary())
            if artifact_type == "deployed_output":
                summary.state = "deployed"
            elif artifact_type == "build_output" and summary.state != "deployed":
                summary.state = "built"
        return summaries

    def _load_item_delivery_summaries(self, job_item_ids: list[int]) -> dict[int, _DeliverySummary]:
        if not job_item_ids:
            return {}

        rows = (
            self.session.query(ArtifactRecord.job_item_id, ArtifactRecord.artifact_type)
            .filter(ArtifactRecord.job_item_id.in_(job_item_ids))
            .order_by(ArtifactRecord.job_item_id.asc(), ArtifactRecord.id.asc())
            .all()
        )

        summaries: dict[int, _DeliverySummary] = {}
        for job_item_id, artifact_type in rows:
            if job_item_id is None:
                continue
            summary = summaries.setdefault(job_item_id, _DeliverySummary())
            if artifact_type == "deployed_output":
                summary.state = "deployed"
            elif artifact_type == "build_output" and summary.state != "deployed":
                summary.state = "built"
        return summaries

    def list_visible_events(self, user_id: int, job_id: int, after_id: int | None, limit: int) -> list[JobEventView]:
        query = (
            self.session.query(JobEventRecord)
            .filter(JobEventRecord.user_id == user_id, JobEventRecord.job_id == job_id)
        )
        if after_id is not None:
            query = query.filter(JobEventRecord.id > after_id)
        rows = query.order_by(JobEventRecord.id.asc()).limit(limit).all()
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

    def list_artifact_summaries(self, user_id: int, job_id: int) -> list[ArtifactSummary]:
        rows = (
            self.session.query(ArtifactRecord)
            .filter(ArtifactRecord.user_id == user_id, ArtifactRecord.job_id == job_id)
            .order_by(ArtifactRecord.id.asc())
            .all()
        )
        return [
            ArtifactSummary(
                id=row.id,
                artifact_type=row.artifact_type,
                storage_provider=row.storage_provider,
                object_key=row.object_key,
                file_name=row.file_name,
                result_summary=row.result_summary,
            )
            for row in rows
        ]
