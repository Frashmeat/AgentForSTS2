from __future__ import annotations

from pathlib import Path

from config import normalize_llm_config

_GLOBAL_PROMPT_HEADER = "## User Configured Global AI Instructions"
_GLOBAL_PROMPT_HEADER_RESOURCE_PATH = Path(__file__).with_name("resources") / "global_prompt_header.txt"


def _load_global_prompt_header() -> str:
    try:
        header = _GLOBAL_PROMPT_HEADER_RESOURCE_PATH.read_text(encoding="utf-8").strip()
    except OSError:
        return _GLOBAL_PROMPT_HEADER
    return header or _GLOBAL_PROMPT_HEADER


def append_global_ai_instructions(base_prompt: str, llm_cfg: dict) -> str:
    custom_prompt = normalize_llm_config(llm_cfg).get("custom_prompt", "").strip()
    if not custom_prompt:
        return base_prompt
    return f"{base_prompt.rstrip()}\n\n{_load_global_prompt_header()}\n{custom_prompt}"
