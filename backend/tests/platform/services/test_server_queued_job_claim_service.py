from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta

from app.modules.platform.application.services.server_queued_job_claim_service import (
    ServerQueuedJobClaimBusyError,
    ServerQueuedJobClaimService,
)


def test_server_queued_job_claim_service_rejects_live_claim(tmp_path):
    service = ServerQueuedJobClaimService(storage_root=tmp_path / "queued-job-claims", lease_seconds=120)

    handle = service.acquire_claim(job_id=11, owner_scope="system_queue_worker")
    payload = json.loads(handle.claim_path.read_text(encoding="utf-8"))

    assert payload["claim_version"] == "v1"
    assert payload["job_id"] == 11
    assert payload["owner_scope"] == "system_queue_worker"
    assert payload["expires_at"] > payload["claimed_at"]

    try:
        service.acquire_claim(job_id=11, owner_scope="system_workspace_resume")
    except ServerQueuedJobClaimBusyError as error:
        assert error.job_id == 11
        assert error.current_holder is not None
        assert error.current_holder.owner_scope == "system_queue_worker"
    else:
        raise AssertionError("expected ServerQueuedJobClaimBusyError when claim holder is still active")


def test_server_queued_job_claim_service_can_take_over_stale_claim(tmp_path):
    service = ServerQueuedJobClaimService(storage_root=tmp_path / "queued-job-claims", lease_seconds=120)
    service.storage_root.mkdir(parents=True, exist_ok=True)
    claim_path = service.storage_root / "11.claim.json"
    claim_path.write_text(
        json.dumps(
            {
                "claim_version": "v1",
                "job_id": 11,
                "owner_scope": "system_queue_worker",
                "claimed_at": (datetime.now(UTC) - timedelta(minutes=10)).isoformat(),
                "expires_at": (datetime.now(UTC) - timedelta(minutes=5)).isoformat(),
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    handle = service.acquire_claim(job_id=11, owner_scope="system_workspace_resume")
    payload = json.loads(handle.claim_path.read_text(encoding="utf-8"))

    assert payload["job_id"] == 11
    assert payload["owner_scope"] == "system_workspace_resume"
    assert not (service.storage_root / "11.claim.recovering.json").exists()
