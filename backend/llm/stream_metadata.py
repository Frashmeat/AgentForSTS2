from __future__ import annotations

from app.shared.infra.llm.agent_backend import resolve_agent_backend_name
from config import normalize_llm_config

DEFAULT_CLAUDE_MODEL = "claude-sonnet-4-6"


def resolve_agent_display_model(llm_cfg: dict | None) -> str:
    cfg = normalize_llm_config(llm_cfg or {})
    model = str(cfg.get("model", "")).strip()
    if model:
        return model

    if cfg.get("mode") == "claude_api":
        return DEFAULT_CLAUDE_MODEL

    backend = resolve_agent_backend_name(cfg)
    if backend == "codex":
        return "Codex CLI 默认模型"
    return "Claude CLI 默认模型"


def resolve_text_display_model(llm_cfg: dict | None) -> str:
    cfg = normalize_llm_config(llm_cfg or {})
    model = str(cfg.get("model", "")).strip()
    if model:
        return model
    return DEFAULT_CLAUDE_MODEL


def build_stream_chunk_payload(
    chunk: str,
    *,
    source: str,
    channel: str = "raw",
    model: str | None = None,
) -> dict[str, str]:
    payload = {
        "chunk": chunk,
        "source": source,
        "channel": channel,
    }
    if model:
        payload["model"] = model
    return payload
