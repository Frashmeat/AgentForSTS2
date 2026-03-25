"""Tests for workflow migration flags and compatibility routing."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.infra.config.settings import normalize_config
from app.shared.infra.feature_flags import resolve_workflow_migration_flags
from routers import batch_workflow, workflow


def test_feature_flags_can_switch_between_legacy_and_modular_workflows():
    default_cfg = normalize_config(None)
    default_flags = resolve_workflow_migration_flags(default_cfg)

    assert default_cfg["migration"] == {
        "use_modular_single_workflow": False,
        "use_modular_batch_workflow": False,
        "use_unified_ws_contract": False,
    }
    assert default_flags.use_modular_single_workflow is False
    assert default_flags.use_modular_batch_workflow is False
    assert default_flags.use_unified_ws_contract is False
    assert workflow._single_workflow_mode(default_cfg) == "legacy"
    assert batch_workflow._batch_workflow_mode(default_cfg) == "legacy"

    modular_cfg = normalize_config(
        {
            "migration": {
                "use_modular_single_workflow": True,
                "use_modular_batch_workflow": True,
                "use_unified_ws_contract": True,
            }
        }
    )
    modular_flags = resolve_workflow_migration_flags(modular_cfg)

    assert modular_flags.use_modular_single_workflow is True
    assert modular_flags.use_modular_batch_workflow is True
    assert modular_flags.use_unified_ws_contract is True
    assert workflow._single_workflow_mode(modular_cfg) == "modular"
    assert batch_workflow._batch_workflow_mode(modular_cfg) == "modular"


def test_feature_flags_support_partial_rollout():
    cfg = normalize_config({"migration": {"use_modular_single_workflow": True}})
    flags = resolve_workflow_migration_flags(cfg)

    assert flags.use_modular_single_workflow is True
    assert flags.use_modular_batch_workflow is False
    assert flags.use_unified_ws_contract is False
