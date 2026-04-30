import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.approval.application.ports import ActionResult
from app.modules.approval.application.services import ApprovalService
from app.modules.approval.infra.in_memory_store import InMemoryApprovalStore
from app.modules.build.application.ports import BuildRequest, BuildResult
from app.modules.build.infra.dotnet_builder import DotnetBuildBackend


class FakeExecutor:
    def __init__(self) -> None:
        self.actions = []

    async def execute_action(self, action):
        self.actions.append(action)
        return ActionResult(success=True, output="ok", metadata={"exit_code": 0})


@pytest.mark.asyncio
async def test_approval_service_uses_executor_port():
    store = InMemoryApprovalStore()
    executor = FakeExecutor()
    service = ApprovalService(store, executor=executor)
    [action] = service.create_requests_from_plan(
        {
            "actions": [
                {
                    "kind": "write_file",
                    "title": "Write card source",
                    "reason": "Need generated source",
                    "payload": {"path": "Cards/TestCard.cs", "content": "class TestCard {}"},
                }
            ]
        },
        source_backend="codex",
        source_workflow="single_asset",
    )
    store.approve_request(action.action_id)

    updated = await service.execute_request(action.action_id)

    assert executor.actions[0].action_id == action.action_id
    assert updated.status == "succeeded"
    assert updated.result == {"output": "ok", "exit_code": 0}


@pytest.mark.asyncio
async def test_dotnet_build_backend_normalizes_runner_result(tmp_path):
    async def fake_runner(command: list[str], cwd: Path) -> BuildResult:
        assert command == ["dotnet", "build"]
        assert cwd == tmp_path
        return BuildResult(success=True, output="build ok", metadata={"exit_code": 0})

    backend = DotnetBuildBackend(runner=fake_runner)

    result = await backend.build(BuildRequest(command=["dotnet", "build"], cwd=tmp_path))

    assert result.success is True
    assert result.output == "build ok"
    assert result.metadata == {"exit_code": 0}
