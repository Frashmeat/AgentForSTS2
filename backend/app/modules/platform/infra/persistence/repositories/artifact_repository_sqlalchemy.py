from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import ArtifactRepository
from app.modules.platform.infra.persistence.models import ArtifactRecord


class ArtifactRepositorySqlAlchemy(ArtifactRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create(self, artifact: ArtifactRecord) -> ArtifactRecord:
        self.session.add(artifact)
        self.session.flush()
        return artifact

    def list_by_job(self, job_id: int) -> list[ArtifactRecord]:
        return self.session.query(ArtifactRecord).filter(ArtifactRecord.job_id == job_id).order_by(ArtifactRecord.id.asc()).all()

    def list_by_job_item(self, job_item_id: int) -> list[ArtifactRecord]:
        return (
            self.session.query(ArtifactRecord)
            .filter(ArtifactRecord.job_item_id == job_item_id)
            .order_by(ArtifactRecord.id.asc())
            .all()
        )

    def list_by_execution(self, execution_id: int) -> list[ArtifactRecord]:
        return (
            self.session.query(ArtifactRecord)
            .filter(ArtifactRecord.ai_execution_id == execution_id)
            .order_by(ArtifactRecord.id.asc())
            .all()
        )

    def find_by_id_for_user(self, artifact_id: int, user_id: int) -> ArtifactRecord | None:
        return (
            self.session.query(ArtifactRecord)
            .filter(
                ArtifactRecord.id == artifact_id,
                ArtifactRecord.user_id == user_id,
                ArtifactRecord.deleted_at.is_(None),
            )
            .one_or_none()
        )
