import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.admin_queries import AdminExecutionDetailView
from app.modules.platform.contracts.admin_commands import CreateServerCredentialCommand, UpdateServerCredentialCommand
from app.modules.platform.contracts.admin_queries import AdminServerCredentialHealthCheckView
from app.modules.platform.contracts.events import JobEventView
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.contracts.job_queries import JobDetailView
from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.modules.platform.contracts.workstation_execution import (
    WorkstationArtifactPayload,
    WorkstationCallbackConfig,
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionEvent,
    WorkstationExecutionPollResult,
)
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus


def test_platform_status_enums_are_frozen_to_fine_grained_values():
    assert JobStatus.DRAFT.value == "draft"
    assert JobStatus.DEFERRED.value == "deferred"
    assert JobStatus.QUOTA_EXHAUSTED.value == "quota_exhausted"
    assert JobItemStatus.DEFERRED.value == "deferred"
    assert JobItemStatus.CANCELLED_AFTER_START.value == "cancelled_after_start"
    assert AIExecutionStatus.COMPLETED_WITH_REFUND.value == "completed_with_refund"


def test_create_job_command_serializes_expected_contract():
    command = CreateJobCommand.model_validate(
        {
            "job_type": "batch_generate",
            "input_summary": "批量生成 2 个资产",
            "workflow_version": "2026.03.31",
            "items": [
                {
                    "item_type": "card",
                    "input_summary": "第一张卡",
                    "input_payload": {"name": "CardA"},
                }
            ],
        }
    )

    payload = command.model_dump()
    assert payload["job_type"] == "batch_generate"
    assert payload["workflow_version"] == "2026.03.31"
    assert payload["items"][0]["input_payload"]["name"] == "CardA"


def test_user_job_detail_view_does_not_expose_execution_fields():
    detail = JobDetailView.model_validate(
        {
            "id": 1,
            "job_type": "single_generate",
            "status": "draft",
            "items": [],
            "artifacts": [],
        }
    )

    payload = detail.model_dump()
    assert "ai_execution_id" not in payload
    assert payload["status"] == "draft"


def test_admin_execution_detail_view_contains_internal_fields():
    detail = AdminExecutionDetailView.model_validate(
        {
            "id": 10,
            "job_id": 1,
            "job_item_id": 2,
            "status": "dispatching",
            "provider": "openai",
            "model": "gpt-5.4",
            "credential_ref": "cred-a",
            "retry_attempt": 1,
            "switched_credential": True,
            "request_idempotency_key": "idem-1",
            "step_protocol_version": "v1",
            "result_schema_version": "v1",
        }
    )

    payload = detail.model_dump()
    assert payload["credential_ref"] == "cred-a"
    assert payload["retry_attempt"] == 1
    assert payload["switched_credential"] is True
    assert payload["request_idempotency_key"] == "idem-1"
    assert payload["step_protocol_version"] == "v1"


def test_job_event_view_can_hide_admin_only_execution_id_for_user_payload():
    event = JobEventView.model_validate(
        {
            "event_id": 100,
            "event_type": "job.started",
            "job_id": 1,
            "job_item_id": 2,
            "ai_execution_id": 99,
            "occurred_at": "2026-03-31T09:00:00Z",
            "payload": {"status": "running"},
        }
    )

    user_payload = event.as_user_payload()
    assert user_payload["job_item_id"] == 2
    assert "ai_execution_id" not in user_payload


def test_step_execution_request_captures_minimal_protocol_fields():
    request = StepExecutionRequest.model_validate(
        {
            "workflow_version": "2026.03.31",
            "step_protocol_version": "v1",
            "step_type": "image.generate",
            "step_id": "step-1",
            "job_id": 1,
            "job_item_id": 2,
            "input_payload": {"prompt": "dark relic"},
            "result_schema_version": "v1",
            "execution_binding": {
                "agent_backend": "codex",
                "provider": "openai",
                "model": "gpt-5.4",
                "credential_ref": "server-credential:1",
                "auth_type": "api_key",
                "credential": "sk-live",
                "base_url": "https://api.openai.com/v1",
            },
        }
    )

    payload = request.model_dump()
    assert payload["step_protocol_version"] == "v1"
    assert payload["input_payload"]["prompt"] == "dark relic"
    assert payload["execution_binding"]["agent_backend"] == "codex"
    assert payload["execution_binding"]["credential_ref"] == "server-credential:1"


def test_workstation_dispatch_request_serializes_execution_binding_without_callback_enabled():
    request = WorkstationExecutionDispatchRequest.model_validate(
        {
            "execution_id": 2203,
            "job_id": 2002,
            "job_item_id": 2103,
            "job_type": "single_generate",
            "item_type": "relic",
            "workflow_version": "2026.03.31",
            "step_protocol_version": "v1",
            "result_schema_version": "v1",
            "input_payload": {
                "item_name": "FangedGrimoire",
                "description": "每次造成伤害时获得 2 点格挡。",
            },
            "execution_binding": {
                "agent_backend": "codex",
                "provider": "openai",
                "model": "gpt-5.4",
                "credential_ref": "server-credential:1",
                "auth_type": "api_key",
                "credential": "sk-live",
                "base_url": "https://api.openai.com/v1",
            },
        }
    )

    payload = request.model_dump()
    assert payload["execution_id"] == 2203
    assert payload["input_payload"]["item_name"] == "FangedGrimoire"
    assert payload["execution_binding"]["credential"] == "sk-live"
    assert payload["callback"] == {
        "enabled": False,
        "url": "",
        "token_ref": "",
        "auth": "none",
        "signature_version": "",
    }


def test_workstation_poll_result_serializes_artifact_contract():
    result = WorkstationExecutionPollResult.model_validate(
        {
            "workstation_execution_id": "ws-exec-2203",
            "status": "succeeded",
            "step_id": "build.project",
            "output_payload": {
                "text": "构建成功",
                "artifacts": [
                    {
                        "artifact_type": "build_output",
                        "storage_provider": "server_workspace",
                        "object_key": "server-workspace:abc/build/mod.pck",
                        "file_name": "mod.pck",
                        "mime_type": "application/octet-stream",
                        "size_bytes": 12345,
                        "result_summary": "构建产物",
                    }
                ],
            },
        }
    )

    payload = result.model_dump()
    assert payload["status"] == "succeeded"
    assert payload["output_payload"]["artifacts"][0] == {
        "artifact_type": "build_output",
        "storage_provider": "server_workspace",
        "object_key": "server-workspace:abc/build/mod.pck",
        "file_name": "mod.pck",
        "mime_type": "application/octet-stream",
        "size_bytes": 12345,
        "result_summary": "构建产物",
    }
    assert payload["error_payload"] == {}
    assert payload["events"] == []


def test_workstation_poll_result_serializes_step_events():
    result = WorkstationExecutionPollResult.model_validate(
        {
            "workstation_execution_id": "ws-exec-2203",
            "status": "running",
            "step_id": "single.custom_code.codegen",
            "events": [
                {
                    "sequence": 1,
                    "event_type": "workstation.step.started",
                    "occurred_at": "2026-04-29T10:00:00+00:00",
                    "payload": {
                        "phase": "code_generation",
                        "step_id": "single.custom_code.codegen",
                        "step_type": "code.generate",
                        "message": "正在生成代码",
                    },
                }
            ],
        }
    )

    payload = result.model_dump()
    assert payload["events"][0] == {
        "sequence": 1,
        "event_type": "workstation.step.started",
        "occurred_at": "2026-04-29T10:00:00+00:00",
        "payload": {
            "phase": "code_generation",
            "step_id": "single.custom_code.codegen",
            "step_type": "code.generate",
            "message": "正在生成代码",
        },
    }


def test_workstation_execution_event_defaults_payload_to_empty():
    event = WorkstationExecutionEvent.model_validate(
        {
            "sequence": 1,
            "event_type": "workstation.step.finished",
            "occurred_at": "2026-04-29T10:00:01+00:00",
        }
    )

    assert event.model_dump()["payload"] == {}


def test_workstation_callback_config_defaults_to_disabled():
    callback = WorkstationCallbackConfig.model_validate({})

    assert callback.model_dump() == {
        "enabled": False,
        "url": "",
        "token_ref": "",
        "auth": "none",
        "signature_version": "",
    }


def test_workstation_artifact_payload_supports_uploaded_asset_provider():
    artifact = WorkstationArtifactPayload.model_validate(
        {
            "artifact_type": "source_image",
            "storage_provider": "uploaded_asset",
            "object_key": "uploaded-asset:abc",
        }
    )

    payload = artifact.model_dump()
    assert payload["artifact_type"] == "source_image"
    assert payload["storage_provider"] == "uploaded_asset"
    assert payload["object_key"] == "uploaded-asset:abc"
    assert payload["file_name"] == ""
    assert payload["size_bytes"] == 0


def test_create_server_credential_command_applies_defaults():
    command = CreateServerCredentialCommand.model_validate(
        {
            "execution_profile_id": 1,
            "provider": "openai",
            "auth_type": "api_key",
            "credential": "sk-live",
        }
    )

    payload = command.model_dump()
    assert payload["secret"] == ""
    assert payload["base_url"] == ""
    assert payload["priority"] == 0
    assert payload["enabled"] is True


def test_update_server_credential_command_applies_optional_secret_defaults():
    command = UpdateServerCredentialCommand.model_validate(
        {
            "execution_profile_id": 1,
            "provider": "openai",
            "auth_type": "api_key",
            "label": "main",
        }
    )

    payload = command.model_dump()
    assert payload["credential"] == ""
    assert payload["secret"] == ""
    assert payload["enabled"] is True


def test_admin_server_credential_health_check_view_serializes_expected_fields():
    view = AdminServerCredentialHealthCheckView.model_validate(
        {
            "credential_id": 1,
            "health_status": "healthy",
            "error_code": "",
            "error_message": "",
            "checked_at": "2026-04-15T01:02:03Z",
        }
    )
    payload = view.model_dump()
    assert payload["credential_id"] == 1
    assert payload["health_status"] == "healthy"
