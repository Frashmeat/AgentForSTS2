from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta

from app.modules.platform.application.services.server_queued_job_scan_claim_service import (
    ServerQueuedJobScanClaimBusyError,
    ServerQueuedJobScanClaimService,
)


def test_server_queued_job_scan_claim_service_rejects_live_claim(tmp_path):
    service = ServerQueuedJobScanClaimService(storage_root=tmp_path / "scan-claims", lease_seconds=10)
    owner_id = "worker-a"

    handle = service.ensure_leadership(owner_id=owner_id, owner_scope="system_queue_worker")
    payload = json.loads(handle.claim_path.read_text(encoding="utf-8"))

    assert payload["claim_version"] == "v1"
    assert payload["leader_epoch"] == 1
    assert payload["owner_id"] == owner_id
    assert payload["owner_scope"] == "system_queue_worker"
    assert payload["expires_at"] > payload["claimed_at"]
    assert payload["renewed_at"] == payload["claimed_at"]

    try:
        service.ensure_leadership(owner_id="worker-b", owner_scope="system_queue_worker")
    except ServerQueuedJobScanClaimBusyError as error:
        assert error.current_holder is not None
        assert error.current_holder.owner_id == owner_id
        assert error.current_holder.owner_scope == "system_queue_worker"
    else:
        raise AssertionError("expected ServerQueuedJobScanClaimBusyError when scan claim is still active")


def test_server_queued_job_scan_claim_service_can_take_over_stale_claim(tmp_path):
    service = ServerQueuedJobScanClaimService(storage_root=tmp_path / "scan-claims", lease_seconds=10)
    service.storage_root.mkdir(parents=True, exist_ok=True)
    claim_path = service.storage_root / "queue-scan.claim.json"
    claim_path.write_text(
        json.dumps(
            {
                "claim_version": "v1",
                "owner_id": "worker-a",
                "owner_scope": "system_queue_worker",
                "claimed_at": (datetime.now(UTC) - timedelta(minutes=1)).isoformat(),
                "renewed_at": (datetime.now(UTC) - timedelta(minutes=1)).isoformat(),
                "expires_at": (datetime.now(UTC) - timedelta(seconds=30)).isoformat(),
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    handle = service.ensure_leadership(owner_id="worker-b", owner_scope="system_queue_worker")
    payload = json.loads(handle.claim_path.read_text(encoding="utf-8"))

    assert payload["leader_epoch"] == 2
    assert payload["owner_id"] == "worker-b"
    assert payload["owner_scope"] == "system_queue_worker"
    assert not (service.storage_root / "queue-scan.claim.recovering.json").exists()


def test_server_queued_job_scan_claim_service_can_renew_existing_leadership(tmp_path):
    service = ServerQueuedJobScanClaimService(storage_root=tmp_path / "scan-claims", lease_seconds=10)

    first = service.ensure_leadership(owner_id="worker-a", owner_scope="system_queue_worker")
    first_payload = json.loads(first.claim_path.read_text(encoding="utf-8"))

    renewed = service.ensure_leadership(
        owner_id="worker-a",
        owner_scope="system_queue_worker",
        current_handle=first,
    )
    renewed_payload = json.loads(renewed.claim_path.read_text(encoding="utf-8"))

    assert renewed_payload["leader_epoch"] == first_payload["leader_epoch"]
    assert renewed_payload["owner_id"] == "worker-a"
    assert renewed_payload["claimed_at"] == first_payload["claimed_at"]
    assert renewed_payload["renewed_at"] >= first_payload["renewed_at"]
    assert renewed_payload["expires_at"] >= first_payload["expires_at"]


def test_server_queued_job_scan_claim_service_returns_current_leader(tmp_path):
    service = ServerQueuedJobScanClaimService(storage_root=tmp_path / "scan-claims", lease_seconds=10)

    service.ensure_leadership(owner_id="worker-a", owner_scope="system_queue_worker")
    holder = service.get_current_leader()

    assert holder is not None
    assert holder.leader_epoch == 1
    assert holder.owner_id == "worker-a"
    assert holder.owner_scope == "system_queue_worker"
