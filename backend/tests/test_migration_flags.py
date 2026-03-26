"""Tests for finalized workflow routing after migration flags removal."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.infra.config.settings import normalize_config
from routers import batch_workflow, workflow


def test_normalize_config_strips_removed_migration_flags():
    cfg = normalize_config(
        {
            "migration": {
                "use_modular_single_workflow": False,
                "use_modular_batch_workflow": False,
                "use_unified_ws_contract": False,
            }
        }
    )

    assert "migration" not in cfg


def test_workflow_routes_always_report_modular_mode():
    default_cfg = normalize_config(None)
    legacy_cfg = normalize_config({"migration": {"use_modular_single_workflow": False}})

    assert workflow._single_workflow_mode(default_cfg) == "modular"
    assert workflow._single_workflow_mode(legacy_cfg) == "modular"
    assert batch_workflow._batch_workflow_mode(default_cfg) == "modular"
    assert batch_workflow._batch_workflow_mode(legacy_cfg) == "modular"
