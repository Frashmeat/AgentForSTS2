from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.runner.workflow_registry import PlatformWorkflowRegistry, PlatformWorkflowStep


def test_workflow_registry_resolves_steps_for_job_and_item_type():
    registry = PlatformWorkflowRegistry(
        {
            ("single_generate", "card"): [
                PlatformWorkflowStep(step_type="image.generate", step_id="image-step"),
                PlatformWorkflowStep(step_type="code.generate", step_id="code-step"),
            ]
        }
    )

    steps = registry.resolve("single_generate", "card")

    assert [step.step_type for step in steps] == ["image.generate", "code.generate"]
    assert steps[1].step_id == "code-step"


def test_workflow_registry_raises_for_unknown_mapping():
    registry = PlatformWorkflowRegistry({})

    with pytest.raises(KeyError):
        registry.resolve("unknown_job", "unknown_item")


def test_workflow_registry_can_resolve_steps_dynamically_from_payload():
    registry = PlatformWorkflowRegistry(
        {
            ("single_generate", "card_fullscreen"): (
                lambda input_payload: (
                    [
                        PlatformWorkflowStep(step_type="asset.generate", step_id="asset-step"),
                        PlatformWorkflowStep(step_type="build.project", step_id="build-step"),
                    ]
                    if input_payload.get("uploaded_asset_ref") and input_payload.get("server_project_ref")
                    else [PlatformWorkflowStep(step_type="single.asset.plan", step_id="plan-step")]
                )
            )
        }
    )

    uploaded_steps = registry.resolve(
        "single_generate",
        "card_fullscreen",
        input_payload={"uploaded_asset_ref": "uploaded-asset:abc", "server_project_ref": "server-workspace:abc"},
    )
    plain_steps = registry.resolve(
        "single_generate",
        "card_fullscreen",
        input_payload={},
    )

    assert [step.step_type for step in uploaded_steps] == ["asset.generate", "build.project"]
    assert [step.step_type for step in plain_steps] == ["single.asset.plan"]
