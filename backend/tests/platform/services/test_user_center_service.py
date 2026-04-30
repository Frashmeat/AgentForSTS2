from __future__ import annotations

import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.auth.domain import UserAccount
from app.modules.platform.application.services import UserCenterService
from app.modules.platform.contracts import JobDetailView, JobItemListItem, JobListItem, UserQuotaView


class FakeAuthService:
    def __init__(self, user: UserAccount) -> None:
        self.user = user

    def get_user_by_id(self, user_id: int) -> UserAccount | None:
        return self.user if self.user.user_id == user_id else None


@dataclass
class FakeJobQueryService:
    quota: UserQuotaView
    jobs: list[JobListItem]
    detail: JobDetailView
    items: list[JobItemListItem]

    def get_quota_view(self, user_id: int, now: datetime):
        return self.quota

    def list_jobs(self, user_id: int):
        return self.jobs

    def get_job_detail(self, user_id: int, job_id: int):
        return self.detail

    def list_job_items(self, user_id: int, job_id: int):
        return self.items


def test_user_center_service_exposes_profile_quota_and_job_views():
    user = UserAccount(
        user_id=1001,
        username="luna",
        email="luna@example.com",
        password_hash="hashed::secret",
        email_verified=True,
        created_at=datetime(2026, 4, 3, 8, 0, tzinfo=UTC),
        email_verified_at=datetime(2026, 4, 3, 8, 30, tzinfo=UTC),
    )
    service = UserCenterService(
        auth_service=FakeAuthService(user),
        job_query_service=FakeJobQueryService(
            quota=UserQuotaView(
                total_limit=10,
                used_amount=3,
                refunded_amount=1,
                adjusted_amount=0,
                remaining=7,
                status="active",
            ),
            jobs=[
                JobListItem(
                    id=101,
                    job_type="single_generate",
                    status="running",
                    input_summary="Dark Relic",
                )
            ],
            detail=JobDetailView(
                id=101,
                job_type="single_generate",
                status="running",
                items=[],
                artifacts=[],
            ),
            items=[
                JobItemListItem(
                    id=201,
                    item_index=0,
                    item_type="card",
                    status="running",
                )
            ],
        ),
    )

    profile = service.get_profile(1001)

    assert profile.username == "luna"
    assert service.get_quota(1001, datetime(2026, 4, 3, 9, 0, tzinfo=UTC)).used_amount == 3
    assert service.list_jobs(1001)[0].job_type == "single_generate"
    assert service.get_job_detail(1001, 101).id == 101
    assert service.list_job_items(1001, 101)[0].item_type == "card"
