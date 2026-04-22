from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta

from app.modules.platform.application.services.server_deploy_target_lock_service import ServerDeployTargetBusyError
from app.modules.platform.application.services.server_deploy_target_lock_service import ServerDeployTargetLockService
from app.modules.platform.application.services.server_workspace_lock_service import ServerWorkspaceBusyError
from app.modules.platform.application.services.server_workspace_lock_service import ServerWorkspaceLockService


def test_server_workspace_lock_service_writes_expiry_metadata_and_rejects_live_holder(tmp_path):
    service = ServerWorkspaceLockService(storage_root=tmp_path / "workspace-locks", lease_seconds=600)

    handle = service.acquire_write_lock(
        server_project_ref="server-workspace:abc123",
        job_id=11,
        job_item_id=22,
        user_id=1001,
    )

    payload = json.loads(handle.lock_path.read_text(encoding="utf-8"))

    assert payload["lock_version"] == "v2"
    assert payload["server_project_ref"] == "server-workspace:abc123"
    assert payload["job_id"] == 11
    assert payload["job_item_id"] == 22
    assert payload["user_id"] == 1001
    assert payload["expires_at"] > payload["locked_at"]

    try:
        service.acquire_write_lock(
            server_project_ref="server-workspace:abc123",
            job_id=33,
            job_item_id=44,
            user_id=1002,
        )
    except ServerWorkspaceBusyError as error:
        assert error.current_holder is not None
        assert error.current_holder.lock_version == "v2"
        assert error.current_holder.job_id == 11
        assert error.current_holder.expires_at == payload["expires_at"]
    else:
        raise AssertionError("expected ServerWorkspaceBusyError when lock holder is still active")


def test_server_workspace_lock_service_can_take_over_stale_lock(tmp_path):
    service = ServerWorkspaceLockService(storage_root=tmp_path / "workspace-locks", lease_seconds=600)
    service.storage_root.mkdir(parents=True, exist_ok=True)
    stale_path = service.storage_root / "abc123.lock.json"
    stale_path.write_text(
        json.dumps(
            {
                "lock_version": "v2",
                "server_project_ref": "server-workspace:abc123",
                "job_id": 1,
                "job_item_id": 2,
                "user_id": 1001,
                "owner_scope": "workspace_write",
                "locked_at": (datetime.now(UTC) - timedelta(minutes=20)).isoformat(),
                "expires_at": (datetime.now(UTC) - timedelta(minutes=10)).isoformat(),
                "resource_key": "server-workspace:abc123",
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    handle = service.acquire_write_lock(
        server_project_ref="server-workspace:abc123",
        job_id=77,
        job_item_id=88,
        user_id=2001,
    )

    payload = json.loads(handle.lock_path.read_text(encoding="utf-8"))

    assert payload["job_id"] == 77
    assert payload["job_item_id"] == 88
    assert payload["user_id"] == 2001
    assert payload["lock_version"] == "v2"
    assert not (service.storage_root / "abc123.lock.recovering.json").exists()


def test_server_workspace_lock_service_keeps_recent_legacy_v1_lock_busy(tmp_path):
    service = ServerWorkspaceLockService(storage_root=tmp_path / "workspace-locks", lease_seconds=600)
    service.storage_root.mkdir(parents=True, exist_ok=True)
    stale_path = service.storage_root / "abc123.lock.json"
    stale_path.write_text(
        json.dumps(
            {
                "server_project_ref": "server-workspace:abc123",
                "job_id": 1,
                "job_item_id": 2,
                "user_id": 1001,
                "owner_scope": "workspace_write",
                "locked_at": (datetime.now(UTC) - timedelta(minutes=1)).isoformat(),
                "resource_key": "server-workspace:abc123",
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    try:
        service.acquire_write_lock(
            server_project_ref="server-workspace:abc123",
            job_id=77,
            job_item_id=88,
            user_id=2001,
        )
    except ServerWorkspaceBusyError as error:
        assert error.current_holder is not None
        assert error.current_holder.lock_version == "v1"
        assert error.current_holder.job_id == 1
    else:
        raise AssertionError("expected ServerWorkspaceBusyError for recent legacy v1 lock")


def test_server_deploy_target_lock_service_can_take_over_stale_lock(tmp_path):
    service = ServerDeployTargetLockService(storage_root=tmp_path / "deploy-locks", lease_seconds=120)
    service.storage_root.mkdir(parents=True, exist_ok=True)
    token = service._token_from_project_name("DarkMod")
    stale_path = service.storage_root / f"{token}.lock.json"
    stale_path.write_text(
        json.dumps(
            {
                "lock_version": "v2",
                "project_name": "DarkMod",
                "job_id": 5,
                "job_item_id": 6,
                "user_id": 1001,
                "owner_scope": "deploy_target_write",
                "locked_at": (datetime.now(UTC) - timedelta(minutes=10)).isoformat(),
                "expires_at": (datetime.now(UTC) - timedelta(minutes=5)).isoformat(),
                "resource_key": "DarkMod",
                "server_project_ref": "server-workspace:old",
                "source_workspace_root": "I:/runtime/workspaces/old",
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    handle = service.acquire_write_lock(
        project_name="DarkMod",
        job_id=105,
        job_item_id=106,
        user_id=2002,
        server_project_ref="server-workspace:new",
        source_workspace_root="I:/runtime/workspaces/new",
    )

    payload = json.loads(handle.lock_path.read_text(encoding="utf-8"))

    assert payload["job_id"] == 105
    assert payload["job_item_id"] == 106
    assert payload["user_id"] == 2002
    assert payload["server_project_ref"] == "server-workspace:new"
    assert payload["lock_version"] == "v2"


def test_server_deploy_target_lock_service_reports_live_holder_expiry_metadata(tmp_path):
    service = ServerDeployTargetLockService(storage_root=tmp_path / "deploy-locks", lease_seconds=120)
    handle = service.acquire_write_lock(
        project_name="DarkMod",
        job_id=11,
        job_item_id=12,
        user_id=1001,
        server_project_ref="server-workspace:abc123",
        source_workspace_root="I:/runtime/workspaces/abc123",
    )

    try:
        service.acquire_write_lock(
            project_name="DarkMod",
            job_id=21,
            job_item_id=22,
            user_id=2001,
            server_project_ref="server-workspace:def456",
            source_workspace_root="I:/runtime/workspaces/def456",
        )
    except ServerDeployTargetBusyError as error:
        payload = error.to_error_payload()
        assert payload["reason_code"] == "server_deploy_target_busy"
        assert payload["current_holder"]["lock_version"] == "v2"
        assert payload["current_holder"]["job_id"] == 11
        assert payload["current_holder"]["expires_at"]
    else:
        raise AssertionError("expected ServerDeployTargetBusyError when deploy holder is still active")

    service.release_write_lock(handle)
