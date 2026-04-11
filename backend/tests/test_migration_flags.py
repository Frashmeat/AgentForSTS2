"""Tests for current workflow routing defaults."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.infra.config.settings import normalize_config


def _read_router_source(name: str) -> str:
    return (Path(__file__).resolve().parents[1] / "routers" / name).read_text(encoding="utf-8")


def test_normalize_config_discards_legacy_migration_flags():
    cfg = normalize_config(
        {
            "migration": {
                "use_modular_single_workflow": True,
                "use_modular_batch_workflow": True,
                "use_unified_ws_contract": True,
                "platform_jobs_api_enabled": True,
            }
        }
    )

    assert cfg["migration"] == {}


def test_workflow_routes_always_report_modular_mode():
    workflow_source = _read_router_source("workflow.py")
    batch_source = _read_router_source("batch_workflow.py")

    assert 'def _single_workflow_mode(config: dict | None = None) -> str:\n    return "modular"' in workflow_source
    assert 'def _batch_workflow_mode(config: dict | None = None) -> str:\n    return "modular"' in batch_source
