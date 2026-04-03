from __future__ import annotations

from app.modules.platform.contracts import ArtifactSummary, JobDetailView, JobEventView, JobItemListItem, JobListItem
from app.modules.platform.domain.repositories import JobQueryRepository
from app.modules.platform.infra.persistence.models import ArtifactRecord, JobEventRecord, JobItemRecord, JobRecord
from sqlalchemy.orm import Session


def _to_iso(value: object | None) -> str:
    if value is None:
        return ""
    return value.isoformat()


def _enum_value(value: object) -> str:
    return value.value if hasattr(value, "value") else str(value)


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
        return [
            JobListItem(
                id=row.id,
                job_type=row.job_type,
                status=_enum_value(row.status),
                input_summary=row.input_summary,
                result_summary=row.result_summary,
                total_item_count=row.total_item_count,
                succeeded_item_count=row.succeeded_item_count,
                failed_item_count=row.failed_business_item_count + row.failed_system_item_count,
            )
            for row in rows
        ]

    def get_job_detail(self, user_id: int, job_id: int) -> JobDetailView | None:
        row = self.session.query(JobRecord).filter(JobRecord.id == job_id, JobRecord.user_id == user_id).one_or_none()
        if row is None:
            return None
        return JobDetailView(
                id=row.id,
                job_type=row.job_type,
                status=_enum_value(row.status),
            input_summary=row.input_summary,
            result_summary=row.result_summary,
            error_summary=row.error_summary,
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
        return [
            JobItemListItem(
                id=row.id,
                item_index=row.item_index,
                item_type=row.item_type,
                status=_enum_value(row.status),
                result_summary=row.result_summary,
                error_summary=row.error_summary,
            )
            for row in rows
        ]

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
                file_name=row.file_name,
                result_summary=row.result_summary,
            )
            for row in rows
        ]
