from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime

from app.modules.auth.application import AuthService
from app.modules.platform.application.services.job_query_service import JobQueryService
from app.modules.platform.contracts._model import ModelBase


@dataclass(slots=True)
class UserCenterProfileView(ModelBase):
    user_id: int
    username: str
    email: str
    email_verified: bool
    created_at: str
    email_verified_at: str | None = None


class UserCenterService:
    def __init__(self, auth_service: AuthService, job_query_service: JobQueryService) -> None:
        self.auth_service = auth_service
        self.job_query_service = job_query_service

    def get_profile(self, user_id: int) -> UserCenterProfileView:
        user = self.auth_service.get_user_by_id(user_id)
        if user is None:
            raise LookupError(f"user not found: {user_id}")
        return UserCenterProfileView(
            user_id=user.user_id,
            username=user.username,
            email=user.email,
            email_verified=user.email_verified,
            created_at=user.created_at.isoformat(),
            email_verified_at=user.email_verified_at.isoformat() if user.email_verified_at else None,
        )

    def get_quota(self, user_id: int, now: datetime):
        return self.job_query_service.get_quota_view(user_id, now)

    def list_jobs(self, user_id: int):
        return self.job_query_service.list_jobs(user_id)

    def get_job_detail(self, user_id: int, job_id: int):
        return self.job_query_service.get_job_detail(user_id, job_id)

    def list_job_items(self, user_id: int, job_id: int):
        return self.job_query_service.list_job_items(user_id, job_id)

    def list_events(self, user_id: int, job_id: int, after_id: int | None = None, limit: int = 50):
        return self.job_query_service.list_events(user_id, job_id, after_id=after_id, limit=limit)
