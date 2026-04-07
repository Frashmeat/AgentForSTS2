from __future__ import annotations

from config import normalize_llm_config

_TEXT_MODEL_MAP = {
    "anthropic": "claude-sonnet-4-6",
    "openai": "openai/gpt-5",
    "moonshot": "moonshot/moonshot-v1-8k",
    "deepseek": "deepseek/deepseek-chat",
    "qwen": "openai/qwen-plus",
    "zhipu": "zhipuai/glm-4-flash",
}


def resolve_agent_display_model(llm_cfg: dict | None) -> str:
    cfg = normalize_llm_config(llm_cfg or {})
    model = str(cfg.get("model", "")).strip()
    if model:
        return model

    backend = cfg.get("agent_backend", "claude")
    if backend == "codex":
        return "Codex CLI 默认模型"
    return "Claude CLI 默认模型"


def resolve_text_display_model(llm_cfg: dict | None) -> str:
    cfg = normalize_llm_config(llm_cfg or {})
    model = str(cfg.get("model", "")).strip()
    if model:
        return model
    provider = cfg.get("provider", "anthropic")
    return _TEXT_MODEL_MAP.get(provider, "claude-sonnet-4-6")


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
