from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping

from app.shared.infra.config.settings import normalize_config
from config import get_config


@dataclass(frozen=True, slots=True)
class WorkflowMigrationFlags:
    use_modular_single_workflow: bool = False
    use_modular_batch_workflow: bool = False
    use_unified_ws_contract: bool = False

    def to_dict(self) -> dict[str, bool]:
        return {
            "use_modular_single_workflow": self.use_modular_single_workflow,
            "use_modular_batch_workflow": self.use_modular_batch_workflow,
            "use_unified_ws_contract": self.use_unified_ws_contract,
        }


@dataclass(frozen=True, slots=True)
class PlatformMigrationFlags:
    platform_jobs_api_enabled: bool = False
    platform_service_split_enabled: bool = False
    platform_runner_enabled: bool = False
    platform_events_v1_enabled: bool = False
    platform_step_protocol_enabled: bool = False

    def to_dict(self) -> dict[str, bool]:
        return {
            "platform_jobs_api_enabled": self.platform_jobs_api_enabled,
            "platform_service_split_enabled": self.platform_service_split_enabled,
            "platform_runner_enabled": self.platform_runner_enabled,
            "platform_events_v1_enabled": self.platform_events_v1_enabled,
            "platform_step_protocol_enabled": self.platform_step_protocol_enabled,
        }


def resolve_workflow_migration_flags(config: Mapping[str, Any] | None = None) -> WorkflowMigrationFlags:
    normalized = normalize_config(dict(config) if config is not None else get_config())
    raw = normalized.get("migration", {})
    return WorkflowMigrationFlags(
        use_modular_single_workflow=bool(raw.get("use_modular_single_workflow", False)),
        use_modular_batch_workflow=bool(raw.get("use_modular_batch_workflow", False)),
        use_unified_ws_contract=bool(raw.get("use_unified_ws_contract", False)),
    )


def resolve_platform_migration_flags(config: Mapping[str, Any] | None = None) -> PlatformMigrationFlags:
    normalized = normalize_config(dict(config) if config is not None else get_config())
    raw = normalized.get("migration", {})
    return PlatformMigrationFlags(
        platform_jobs_api_enabled=bool(raw.get("platform_jobs_api_enabled", False)),
        platform_service_split_enabled=bool(raw.get("platform_service_split_enabled", False)),
        platform_runner_enabled=bool(raw.get("platform_runner_enabled", False)),
        platform_events_v1_enabled=bool(raw.get("platform_events_v1_enabled", False)),
        platform_step_protocol_enabled=bool(raw.get("platform_step_protocol_enabled", False)),
    )
