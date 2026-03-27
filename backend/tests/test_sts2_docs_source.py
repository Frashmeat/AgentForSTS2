import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra.sts2_docs_source import Sts2DocsKnowledgeSource


def test_sts2_docs_source_uses_planner_hints_for_planner_context():
    source = Sts2DocsKnowledgeSource(
        docs_for_type=lambda asset_type: f"docs:{asset_type}",
        planner_hints=lambda: "planner-hints",
    )

    assert source.load_context("planner") == "planner-hints"


def test_sts2_docs_source_uses_asset_type_for_asset_context():
    source = Sts2DocsKnowledgeSource(
        docs_for_type=lambda asset_type: f"docs:{asset_type}",
        planner_hints=lambda: "planner-hints",
    )

    assert source.load_context("asset", asset_type="power") == "docs:power"


def test_sts2_docs_source_returns_empty_string_without_asset_type():
    source = Sts2DocsKnowledgeSource(
        docs_for_type=lambda asset_type: f"docs:{asset_type}",
        planner_hints=lambda: "planner-hints",
    )

    assert source.load_context("asset") == ""
