from __future__ import annotations

from app.shared.prompting import PromptLoader
from config import normalize_llm_config

_PROMPT_LOADER = PromptLoader()
_GLOBAL_PROMPT_HEADER_BUNDLE_KEY = "runtime_agent.llm_global_prompt_header"


def _load_global_prompt_header() -> str:
    return _PROMPT_LOADER.load(_GLOBAL_PROMPT_HEADER_BUNDLE_KEY).strip()


def append_global_ai_instructions(base_prompt: str, llm_cfg: dict) -> str:
    custom_prompt = normalize_llm_config(llm_cfg).get("custom_prompt", "").strip()
    if not custom_prompt:
        return base_prompt
    return f"{base_prompt.rstrip()}\n\n{_load_global_prompt_header()}\n{custom_prompt}"
