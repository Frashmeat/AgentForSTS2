"""Tests for prompt builder resource loading."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.prompting import PromptLoader
from llm import prompt_builder


def test_append_global_ai_instructions_uses_shared_bundle_header():
    prompt = prompt_builder.append_global_ai_instructions(
        "base prompt",
        {"custom_prompt": "stay focused"},
    )

    expected_header = PromptLoader().load("llm.global_prompt_header").strip()
    assert prompt == f"base prompt\n\n{expected_header}\nstay focused"


def test_append_global_ai_instructions_falls_back_when_resource_missing(monkeypatch):
    monkeypatch.setattr(
        prompt_builder,
        "_PROMPT_LOADER",
        PromptLoader(root=Path(__file__).parent / "missing-prompts"),
    )
    monkeypatch.setattr(prompt_builder, "_GLOBAL_PROMPT_HEADER", "fallback-header")

    prompt = prompt_builder.append_global_ai_instructions(
        "base prompt",
        {"custom_prompt": "stay focused"},
    )

    assert prompt == "base prompt\n\nfallback-header\nstay focused"
