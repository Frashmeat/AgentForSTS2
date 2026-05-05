from __future__ import annotations

import re
from pathlib import Path

from app.modules.platform.domain.repositories import AIExecutionRepository, ArtifactRepository
from app.modules.platform.infra.persistence.models import ArtifactRecord

_SAFE_NAME_PATTERN = re.compile(r"[^A-Za-z0-9._-]+")


def runtime_artifact_root() -> Path:
    return Path(__file__).resolve().parents[6] / "runtime" / "platform-artifacts"


def safe_artifact_stem(value: object, fallback: str = "result") -> str:
    normalized = _SAFE_NAME_PATTERN.sub("-", str(value or "").strip()).strip(".-")
    return normalized[:80] or fallback


def create_plan_markdown_artifact(
    *,
    job_id: int,
    job_item_id: int,
    user_id: int,
    ai_execution_id: int | None,
    asset_type: str,
    item_name: str,
    summary: str,
    analysis: str,
    artifact_root: Path | None = None,
) -> ArtifactRecord | None:
    content = str(analysis or "").strip()
    if not content:
        return None

    root = artifact_root or runtime_artifact_root()
    artifact_dir = root / "jobs" / str(job_id) / "items" / str(job_item_id)
    artifact_dir.mkdir(parents=True, exist_ok=True)

    item_stem = safe_artifact_stem(item_name, fallback=f"item-{job_item_id}")
    type_stem = safe_artifact_stem(asset_type, fallback="asset")
    file_name = f"{item_stem}.{type_stem}.plan.md"
    path = artifact_dir / file_name
    path.write_text(_render_markdown(asset_type=asset_type, item_name=item_name, summary=summary, analysis=content), encoding="utf-8")

    return ArtifactRecord(
        job_id=job_id,
        job_item_id=job_item_id,
        ai_execution_id=ai_execution_id,
        user_id=user_id,
        artifact_type="plan_markdown",
        storage_provider="server_workspace",
        object_key=str(path),
        file_name=file_name,
        mime_type="text/markdown; charset=utf-8",
        size_bytes=path.stat().st_size,
        result_summary="服务器生成方案文档",
    )


class PlanArtifactBackfillService:
    def __init__(
        self,
        *,
        artifact_repository: ArtifactRepository,
        ai_execution_repository: AIExecutionRepository,
    ) -> None:
        self.artifact_repository = artifact_repository
        self.ai_execution_repository = ai_execution_repository

    def backfill_latest_plan_markdown(self, *, user_id: int, job_id: int) -> ArtifactRecord | None:
        if self.artifact_repository.exists_by_job_type_for_user(user_id, job_id, "plan_markdown"):
            return None

        execution = self.ai_execution_repository.find_latest_succeeded_by_job(user_id, job_id)
        if execution is None:
            return None

        payload = dict(execution.result_payload or {})
        artifact = create_plan_markdown_artifact(
            job_id=job_id,
            job_item_id=int(execution.job_item_id or 0),
            user_id=user_id,
            ai_execution_id=execution.id,
            asset_type=str(payload.get("asset_type", "")).strip(),
            item_name=str(payload.get("item_name", "")).strip(),
            summary=execution.result_summary,
            analysis=str(payload.get("analysis", "")).strip(),
        )
        if artifact is None:
            return None
        return self.artifact_repository.create(artifact)


def _render_markdown(*, asset_type: str, item_name: str, summary: str, analysis: str) -> str:
    lines = [
        f"# {item_name or '生成结果'}",
        "",
        f"- 资产类型：{asset_type or 'unknown'}",
        f"- 摘要：{summary or '无'}",
        "",
        "## 方案内容",
        "",
        analysis,
        "",
    ]
    return "\n".join(lines)
