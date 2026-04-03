import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.shared.infra.feature_flags import (
    PlatformMigrationFlags,
    resolve_platform_migration_flags,
    resolve_workflow_migration_flags,
)
from app.shared.infra.config.settings import normalize_config


def test_platform_migration_flags_default_to_disabled():
    flags = resolve_platform_migration_flags({"migration": {}})

    assert flags == PlatformMigrationFlags()
    assert flags.to_dict() == {
        "platform_jobs_api_enabled": False,
        "platform_service_split_enabled": False,
        "platform_runner_enabled": False,
        "platform_events_v1_enabled": False,
        "platform_step_protocol_enabled": False,
    }


def test_platform_migration_flags_can_be_enabled_without_breaking_workflow_flags():
    cfg = normalize_config(
        {
            "migration": {
                "use_modular_single_workflow": True,
                "platform_jobs_api_enabled": True,
                "platform_runner_enabled": True,
            }
        }
    )

    workflow_flags = resolve_workflow_migration_flags(cfg)
    platform_flags = resolve_platform_migration_flags(cfg)

    assert workflow_flags.use_modular_single_workflow is True
    assert workflow_flags.use_modular_batch_workflow is False
    assert platform_flags.platform_jobs_api_enabled is True
    assert platform_flags.platform_runner_enabled is True
    assert platform_flags.platform_service_split_enabled is False
