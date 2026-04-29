from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import ArtifactRecord


class ArtifactRepository(ABC):
    @abstractmethod
    def create(self, artifact: ArtifactRecord) -> ArtifactRecord: ...

    @abstractmethod
    def list_by_job(self, job_id: int) -> list[ArtifactRecord]: ...

    @abstractmethod
    def list_by_job_item(self, job_item_id: int) -> list[ArtifactRecord]: ...

    @abstractmethod
    def list_by_execution(self, execution_id: int) -> list[ArtifactRecord]: ...

    @abstractmethod
    def find_by_id_for_user(self, artifact_id: int, user_id: int) -> ArtifactRecord | None: ...
