import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra.sts2_guidance_source import Sts2GuidanceKnowledgeSource


def test_sts2_guidance_source_uses_planner_guidance_for_planner_context():
    source = Sts2GuidanceKnowledgeSource(
        guidance_for_asset_type=lambda asset_type: f"guidance:{asset_type}",
        planner_guidance=lambda: "planner-guidance",
    )

    assert source.load_context("planner") == "planner-guidance"


def test_sts2_guidance_source_uses_asset_type_for_asset_context():
    source = Sts2GuidanceKnowledgeSource(
        guidance_for_asset_type=lambda asset_type: f"guidance:{asset_type}",
        planner_guidance=lambda: "planner-guidance",
    )

    assert source.load_context("asset", asset_type="power") == "guidance:power"


def test_sts2_guidance_source_returns_empty_string_without_asset_type():
    source = Sts2GuidanceKnowledgeSource(
        guidance_for_asset_type=lambda asset_type: f"guidance:{asset_type}",
        planner_guidance=lambda: "planner-guidance",
    )

    assert source.load_context("asset") == ""
