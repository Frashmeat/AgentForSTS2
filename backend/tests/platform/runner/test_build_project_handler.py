from __future__ import annotations

import asyncio
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.server_deploy_target_lock_service import (
    ServerDeployTargetBusyError,
)
from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionRequest
from app.modules.platform.runner.build_project_handler import execute_build_project_step


class _BusyDeployTargetLockService:
    def acquire_write_lock(self, **kwargs):
        raise ServerDeployTargetBusyError(
            "server deploy target is busy",
            project_name=str(kwargs.get("project_name", "")).strip() or "DarkMod",
            server_project_ref=str(kwargs.get("server_project_ref", "")).strip(),
            source_workspace_root=str(kwargs.get("source_workspace_root", "")).strip(),
        )

    def release_write_lock(self, handle):
        raise AssertionError("release_write_lock should not be called when acquire fails")


def test_execute_build_project_step_runs_agent_with_execution_binding(tmp_path):
    captured: dict[str, object] = {}
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    output_dir = workspace_root / "bin" / "Release"
    output_dir.mkdir(parents=True)
    (output_dir / "DarkMod.dll").write_bytes(b"dll")
    (output_dir / "DarkMod.pck").write_bytes(b"pck")

    def fake_prompt_builder(max_attempts: int) -> str:
        captured["max_attempts"] = max_attempts
        return f"prompt:build:{max_attempts}"

    async def fake_build_agent_runner(prompt, project_root, llm_cfg):
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        captured["llm_cfg"] = dict(llm_cfg)
        return "Summary: 已完成 BattleScriptManager 的服务器项目构建\nBuild succeeded"

    result = asyncio.run(
        execute_build_project_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="build.project",
                step_id="batch.custom_code.build",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "BattleScriptManager",
                    "server_workspace_root": str(workspace_root),
                },
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                    base_url="https://api.openai.com/v1",
                ),
            ),
            prompt_builder=fake_prompt_builder,
            build_agent_runner=fake_build_agent_runner,
            config_loader=lambda: {"sts2_path": ""},
            ensure_local_props_fn=lambda project_root: True,
        )
    )

    assert captured["max_attempts"] == 3
    assert captured["prompt"] == "prompt:build:3"
    assert captured["project_root"] == workspace_root
    assert captured["llm_cfg"]["agent_backend"] == "codex"
    assert result["text"] == "已完成 BattleScriptManager 的服务器项目构建"
    assert result["item_name"] == "BattleScriptManager"
    assert [artifact["file_name"] for artifact in result["artifacts"]] == ["DarkMod.dll", "DarkMod.pck"]
    assert result["deployed_to"] is None
    assert result["files"] == []


def test_execute_build_project_step_can_deploy_outputs_when_server_game_path_exists(tmp_path):
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    output_dir = workspace_root / "bin" / "Release"
    output_dir.mkdir(parents=True)
    (output_dir / "DarkMod.dll").write_bytes(b"dll")
    (output_dir / "DarkMod.pck").write_bytes(b"pck")
    game_root = tmp_path / "Game"
    (game_root / "Mods").mkdir(parents=True)

    async def fake_build_agent_runner(prompt, project_root, llm_cfg):
        return "Summary: 已完成 BattleScriptManager 的服务器项目构建\nBuild succeeded"

    result = asyncio.run(
        execute_build_project_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="build.project",
                step_id="batch.custom_code.build",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "BattleScriptManager",
                    "server_workspace_root": str(workspace_root),
                    "server_project_ref": "server-workspace:abc123",
                    "runtime_user_id": 1001,
                },
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                ),
            ),
            prompt_builder=lambda attempts: f"prompt:{attempts}",
            build_agent_runner=fake_build_agent_runner,
            config_loader=lambda: {"sts2_path": str(game_root)},
            ensure_local_props_fn=lambda project_root: True,
        )
    )

    assert result["deployed_to"] == str(game_root / "Mods" / "DarkMod")
    assert result["files"] == ["DarkMod.dll", "DarkMod.pck"]
    assert result["text"] == f"已完成 BattleScriptManager 的服务器构建并部署到 {game_root / 'Mods' / 'DarkMod'}"
    artifact_types = [artifact["artifact_type"] for artifact in result["artifacts"]]
    assert artifact_types == ["build_output", "build_output", "deployed_output", "deployed_output"]
    metadata = json.loads((game_root / "Mods" / "DarkMod" / ".server-deploy.json").read_text(encoding="utf-8"))
    assert metadata["schema_version"] == "v1"
    assert metadata["project_name"] == "DarkMod"
    assert metadata["job_id"] == 1
    assert metadata["job_item_id"] == 2
    assert metadata["user_id"] == 1001
    assert metadata["server_project_ref"] == "server-workspace:abc123"
    assert metadata["source_workspace_root"] == str(workspace_root)
    assert metadata["deployed_to"] == str(game_root / "Mods" / "DarkMod")
    assert metadata["entrypoint"] == "platform.build.project"
    assert metadata["file_names"] == ["DarkMod.dll", "DarkMod.pck"]
    assert result["last_successful_deploy"]["project_name"] == "DarkMod"
    assert result["last_successful_deploy"]["job_id"] == 1
    assert result["last_successful_deploy"]["entrypoint"] == "platform.build.project"
    assert result["deploy_recovery_context"]["same_server_project_ref"] is True
    assert result["deploy_recovery_context"]["same_source_workspace_root"] is True


def test_execute_build_project_step_overwrites_existing_target_outputs_instead_of_reusing_old_files(tmp_path):
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    output_dir = workspace_root / "bin" / "Release"
    output_dir.mkdir(parents=True)
    (output_dir / "DarkMod.dll").write_bytes(b"new-dll")
    (output_dir / "DarkMod.pck").write_bytes(b"new-pck")
    game_root = tmp_path / "Game"
    target_dir = game_root / "Mods" / "DarkMod"
    target_dir.mkdir(parents=True)
    (target_dir / "DarkMod.dll").write_bytes(b"old-dll")
    (target_dir / "DarkMod.pck").write_bytes(b"old-pck")

    async def fake_build_agent_runner(prompt, project_root, llm_cfg):
        return "Summary: 已完成 BattleScriptManager 的服务器项目构建\nBuild succeeded"

    result = asyncio.run(
        execute_build_project_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="build.project",
                step_id="batch.custom_code.build",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "BattleScriptManager",
                    "server_workspace_root": str(workspace_root),
                    "server_project_name": "DarkMod",
                    "runtime_user_id": 1001,
                },
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential="sk-live-openai",
                ),
            ),
            prompt_builder=lambda attempts: f"prompt:{attempts}",
            build_agent_runner=fake_build_agent_runner,
            config_loader=lambda: {"sts2_path": str(game_root)},
            ensure_local_props_fn=lambda project_root: True,
        )
    )

    assert result["deployed_to"] == str(target_dir)
    assert (target_dir / "DarkMod.dll").read_bytes() == b"new-dll"
    assert (target_dir / "DarkMod.pck").read_bytes() == b"new-pck"


def test_execute_build_project_step_raises_when_server_deploy_target_is_busy(tmp_path):
    workspace_root = tmp_path / "DarkMod"
    workspace_root.mkdir()
    output_dir = workspace_root / "bin" / "Release"
    output_dir.mkdir(parents=True)
    (output_dir / "DarkMod.dll").write_bytes(b"dll")
    (output_dir / "DarkMod.pck").write_bytes(b"pck")
    game_root = tmp_path / "Game"
    target_dir = game_root / "Mods" / "DarkMod"
    target_dir.mkdir(parents=True)
    (target_dir / ".server-deploy.json").write_text(
        json.dumps(
            {
                "schema_version": "v1",
                "project_name": "DarkMod",
                "job_id": 99,
                "job_item_id": 199,
                "user_id": 1002,
                "server_project_ref": "server-workspace:old",
                "source_workspace_root": "I:/runtime/workspaces/old",
                "deployed_at": "2026-04-20T10:00:00+00:00",
                "deployed_to": str(target_dir),
                "entrypoint": "legacy.ws.build_deploy",
                "file_names": ["DarkMod.dll", "DarkMod.pck"],
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )

    async def fake_build_agent_runner(prompt, project_root, llm_cfg):
        return "Summary: 已完成 BattleScriptManager 的服务器项目构建\nBuild succeeded"

    try:
        asyncio.run(
            execute_build_project_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="build.project",
                    step_id="batch.custom_code.build",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={
                        "item_name": "BattleScriptManager",
                        "server_workspace_root": str(workspace_root),
                        "server_project_ref": "server-workspace:abc123",
                        "server_project_name": "DarkMod",
                        "runtime_user_id": 1001,
                    },
                    execution_binding=StepExecutionBinding(
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                ),
                prompt_builder=lambda attempts: f"prompt:{attempts}",
                build_agent_runner=fake_build_agent_runner,
                config_loader=lambda: {"sts2_path": str(game_root)},
                ensure_local_props_fn=lambda project_root: True,
                deploy_target_lock_service=_BusyDeployTargetLockService(),
            )
        )
    except ServerDeployTargetBusyError as error:
        assert str(error) == "server deploy target is busy"
        assert error.to_error_payload()["reason_code"] == "server_deploy_target_busy"
        assert error.to_error_payload()["resource_key"] == "DarkMod"
        assert error.to_error_payload()["last_successful_deploy"]["job_id"] == 99
        assert error.to_error_payload()["last_successful_deploy"]["entrypoint"] == "legacy.ws.build_deploy"
        assert error.to_error_payload()["recovery_context"]["same_server_project_ref"] is False
    else:
        raise AssertionError("expected ServerDeployTargetBusyError when deploy target lock cannot be acquired")


def test_execute_build_project_step_requires_server_workspace_root():
    try:
        asyncio.run(
            execute_build_project_step(
                StepExecutionRequest(
                    workflow_version="2026.03.31",
                    step_protocol_version="v1",
                    step_type="build.project",
                    step_id="batch.custom_code.build",
                    job_id=1,
                    job_item_id=2,
                    result_schema_version="v1",
                    input_payload={"item_name": "BattleScriptManager"},
                    execution_binding=StepExecutionBinding(
                        agent_backend="codex",
                        provider="openai",
                        model="gpt-5.4",
                        credential="sk-live-openai",
                    ),
                )
            )
        )
    except ValueError as error:
        assert str(error) == "build.project requires server_workspace_root"
    else:
        raise AssertionError("expected ValueError when server_workspace_root is missing")
