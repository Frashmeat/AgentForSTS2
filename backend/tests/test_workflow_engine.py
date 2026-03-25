import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.workflow.application.context import WorkflowContext
from app.modules.workflow.application.engine import WorkflowEngine
from app.modules.workflow.application.policies import DagExecutionPolicy, LimitedParallelPolicy
from app.modules.workflow.application.step import WorkflowStep


@pytest.mark.asyncio
async def test_workflow_engine_executes_steps_and_emits_events():
    events = []

    async def publish(event):
        events.append(event)

    async def step_a(context: WorkflowContext):
        context.set("value", "a")
        return "a"

    async def step_b(context: WorkflowContext):
        return f"{context.get('value')}-b"

    engine = WorkflowEngine(policy=DagExecutionPolicy(), publisher=publish)
    context = WorkflowContext()

    result = await engine.execute(
        [
            WorkflowStep(name="step_b", handler=step_b, depends_on=["step_a"]),
            WorkflowStep(name="step_a", handler=step_a),
        ],
        context,
    )

    assert result.get("step_a") == "a"
    assert result.get("step_b") == "a-b"
    assert [event.stage for event in events] == ["step_a", "step_a", "step_b", "step_b"]


@pytest.mark.asyncio
async def test_workflow_engine_stops_on_error_and_emits_error_event():
    events = []

    async def publish(event):
        events.append(event)

    async def bad_step(context: WorkflowContext):
        raise RuntimeError("boom")

    engine = WorkflowEngine(publisher=publish)

    with pytest.raises(RuntimeError):
        await engine.execute([WorkflowStep(name="explode", handler=bad_step)], WorkflowContext())

    assert events[-1].stage == "error"


@pytest.mark.asyncio
async def test_limited_parallel_policy_exposes_concurrency_limit():
    policy = LimitedParallelPolicy(max_concurrency=2)

    semaphore = policy.semaphore()

    assert semaphore._value == 2
