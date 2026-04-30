from __future__ import annotations

import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.application.workstation_platform_executor import (
    WorkstationPlatformExecutor,
    build_workstation_workflow_registry,
)
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.contracts.workstation_execution import WorkstationExecutionDispatchRequest


class FakeRunner:
    def __init__(self, output_payload: dict[str, object] | None = None) -> None:
        self._output_payload = output_payload or {"text": "ok"}
        self.steps = []
        self.base_request = None

    async def run(self, *, steps, base_request, event_publisher=None):
        self.steps = steps
        self.base_request = base_request
        for step in steps[:-1]:
            if event_publisher is not None:
                event_publisher("step.started", step.step_id)
                event_publisher("step.finished", step.step_id)
        if event_publisher is not None:
            event_publisher("step.started", steps[-1].step_id)
            event_publisher("step.finished", steps[-1].step_id)
        return [
            StepExecutionResult(
                step_id=steps[-1].step_id,
                status="succeeded",
                output_payload=dict(self._output_payload),
            )
        ]


class LegacyFakeRunner:
    def __init__(self) -> None:
        self.steps = []
        self.base_request = None

    async def run(self, *, steps, base_request, event_publisher=None):
        self.steps = steps
        self.base_request = base_request
        for step in steps:
            if event_publisher is not None:
                event_publisher("step.started", step.step_id)
                event_publisher("step.finished", step.step_id)
        return [
            StepExecutionResult(
                step_id=steps[-1].step_id,
                status="succeeded",
                output_payload={"text": "ok"},
            )
        ]


def _dispatch_request(
    job_type: str = "single_generate", item_type: str = "relic"
) -> WorkstationExecutionDispatchRequest:
    return WorkstationExecutionDispatchRequest.model_validate(
        {
            "execution_id": 2203,
            "job_id": 2002,
            "job_item_id": 2103,
            "job_type": job_type,
            "item_type": item_type,
            "workflow_version": "2026.03.31",
            "step_protocol_version": "v1",
            "result_schema_version": "v1",
            "input_payload": {"description": "生成内容"},
            "execution_binding": {
                "provider": "openai",
                "model": "gpt-5.4",
                "credential_ref": "server-credential:1",
                "credential": "sk-live",
            },
        }
    )


def test_workstation_platform_executor_runs_text_workflow_and_returns_poll_result():
    runner = LegacyFakeRunner()
    executor = WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=runner,
    )

    result = executor.execute(_dispatch_request("single_generate", "relic")).model_dump()

    assert [step.step_type for step in runner.steps] == ["single.asset.plan"]
    assert result == {
        "workstation_execution_id": "ws-exec-2203",
        "status": "succeeded",
        "step_id": "single.relic.plan",
        "output_payload": {"text": "ok"},
        "error_summary": "",
        "error_payload": {},
        "events": [
            {
                "sequence": 1,
                "event_type": "workstation.step.started",
                "occurred_at": result["events"][0]["occurred_at"],
                "payload": {
                    "phase": "planning",
                    "step_id": "single.relic.plan",
                    "step_type": "single.asset.plan",
                    "message": "正在生成方案",
                },
            },
            {
                "sequence": 2,
                "event_type": "workstation.step.finished",
                "occurred_at": result["events"][1]["occurred_at"],
                "payload": {
                    "phase": "planning",
                    "step_id": "single.relic.plan",
                    "step_type": "single.asset.plan",
                    "message": "已完成当前步骤",
                },
            },
        ],
    }


def test_workstation_platform_executor_runs_code_workflow_steps():
    runner = LegacyFakeRunner()
    executor = WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=runner,
    )

    result = executor.execute(_dispatch_request("single_generate", "custom_code")).model_dump()

    assert [step.step_type for step in runner.steps] == [
        "batch.custom_code.plan",
        "code.generate",
    ]
    assert result["status"] == "succeeded"
    assert [event["event_type"] for event in result["events"]] == [
        "workstation.step.started",
        "workstation.step.finished",
        "workstation.step.started",
        "workstation.step.finished",
    ]


def test_workstation_platform_executor_does_not_build_card_fullscreen_on_server():
    runner = LegacyFakeRunner()
    executor = WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=runner,
    )
    request = _dispatch_request("single_generate", "card_fullscreen")
    request.input_payload["uploaded_asset_ref"] = "uploaded-asset:a"
    request.input_payload["server_project_ref"] = "server-workspace:a"

    result = executor.execute(request).model_dump()

    assert [step.step_type for step in runner.steps] == ["asset.generate"]
    assert result["status"] == "succeeded"


def test_workstation_platform_executor_adds_source_project_artifact(tmp_path):
    project_root = tmp_path / "GeneratedMod"
    project_root.mkdir()
    (project_root / "GeneratedMod.csproj").write_text("<Project />\n", encoding="utf-8")
    (project_root / "bin").mkdir()
    (project_root / "bin" / "ignored.dll").write_text("binary\n", encoding="utf-8")
    runner = FakeRunner({"text": "ok", "server_workspace_root": str(project_root)})
    executor = WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=runner,
    )

    result = executor.execute(_dispatch_request("single_generate", "custom_code")).model_dump()

    artifact = result["output_payload"]["artifacts"][0]
    assert artifact["artifact_type"] == "source_project"
    assert artifact["storage_provider"] == "server_workspace"
    assert artifact["file_name"] == "GeneratedMod.source.zip"
    zip_path = Path(str(artifact["object_key"]))
    assert zip_path.exists()
    with zipfile.ZipFile(zip_path) as archive:
        assert "GeneratedMod.csproj" in archive.namelist()
        assert "bin/ignored.dll" not in archive.namelist()
