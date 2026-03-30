from __future__ import annotations

from app.shared.prompting import PromptLoader
from config import normalize_llm_config

_GLOBAL_PROMPT_HEADER = "## User Configured Global AI Instructions"
_PROMPT_LOADER = PromptLoader()
_GLOBAL_PROMPT_HEADER_BUNDLE_KEY = "llm.global_prompt_header"


def _load_global_prompt_header() -> str:
    header = _PROMPT_LOADER.load(
        _GLOBAL_PROMPT_HEADER_BUNDLE_KEY,
        fallback_template=_GLOBAL_PROMPT_HEADER,
    ).strip()
    return header or _GLOBAL_PROMPT_HEADER


def append_global_ai_instructions(base_prompt: str, llm_cfg: dict) -> str:
    custom_prompt = normalize_llm_config(llm_cfg).get("custom_prompt", "").strip()
    if not custom_prompt:
        return base_prompt
    return f"{base_prompt.rstrip()}\n\n{_load_global_prompt_header()}\n{custom_prompt}"
