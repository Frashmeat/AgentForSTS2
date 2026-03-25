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


def resolve_workflow_migration_flags(config: Mapping[str, Any] | None = None) -> WorkflowMigrationFlags:
    normalized = normalize_config(dict(config) if config is not None else get_config())
    raw = normalized.get("migration", {})
    return WorkflowMigrationFlags(
        use_modular_single_workflow=bool(raw.get("use_modular_single_workflow", False)),
        use_modular_batch_workflow=bool(raw.get("use_modular_batch_workflow", False)),
        use_unified_ws_contract=bool(raw.get("use_unified_ws_contract", False)),
    )
