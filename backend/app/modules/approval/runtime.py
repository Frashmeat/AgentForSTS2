from __future__ import annotations

from pathlib import Path

from app.modules.approval.application.services import ApprovalService
from app.modules.approval.infra.in_memory_store import InMemoryApprovalStore
from app.modules.approval.infra.local_executor import LocalApprovalExecutor
from config import get_config

_store: InMemoryApprovalStore | None = None
_service: ApprovalService | None = None
_executor: LocalApprovalExecutor | None = None


def _repo_root() -> Path:
    # backend/app/modules/approval/runtime.py -> backend -> repo root
    return Path(__file__).resolve().parents[4]


def _configured_allowed_roots() -> list[Path]:
    approval_cfg = get_config().get("approval", {})
    configured_roots = approval_cfg.get("allowed_roots", [])
    repo_root = _repo_root()

    roots: list[Path] = []
    for raw_root in configured_roots:
        candidate = Path(str(raw_root))
        if not candidate.is_absolute():
            candidate = repo_root / candidate
        roots.append(candidate.resolve())

    return roots or [repo_root]


def _configured_allowed_commands() -> list[list[str]]:
    approval_cfg = get_config().get("approval", {})
    configured_commands = approval_cfg.get("allowed_commands", [])

    normalized: list[list[str]] = []
    for raw_command in configured_commands:
        if isinstance(raw_command, str):
            if raw_command:
                normalized.append([raw_command])
            continue
        if isinstance(raw_command, list | tuple):
            command = [str(part) for part in raw_command if str(part)]
            if command:
                normalized.append(command)

    return normalized


def _build_executor() -> LocalApprovalExecutor:
    return LocalApprovalExecutor(
        allowed_roots=_configured_allowed_roots(),
        allowed_commands=_configured_allowed_commands(),
    )


def get_approval_store() -> InMemoryApprovalStore:
    global _store
    if _store is None:
        _store = InMemoryApprovalStore()
    return _store


def get_approval_service() -> ApprovalService:
    global _service
    if _service is None:
        _service = ApprovalService(get_approval_store(), get_approval_executor())
    return _service


def get_approval_executor() -> LocalApprovalExecutor:
    global _executor
    if _executor is None:
        _executor = _build_executor()
    return _executor


def reset_approval_runtime() -> None:
    global _store, _service, _executor
    _store = InMemoryApprovalStore()
    _executor = _build_executor()
    _service = ApprovalService(_store, _executor)
