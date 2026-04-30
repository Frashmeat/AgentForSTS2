"""Tests for prompt builder resource loading."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.prompting import PromptLoader, PromptNotFoundError
from llm import prompt_builder


def test_append_global_ai_instructions_uses_shared_bundle_header():
    prompt = prompt_builder.append_global_ai_instructions(
        "base prompt",
        {"custom_prompt": "stay focused"},
    )

    expected_header = PromptLoader().load("runtime_agent.llm_global_prompt_header").strip()
    assert prompt == f"base prompt\n\n{expected_header}\nstay focused"


def test_append_global_ai_instructions_raises_when_bundle_resource_missing(monkeypatch):
    monkeypatch.setattr(
        prompt_builder,
        "_PROMPT_LOADER",
        PromptLoader(root=Path(__file__).parent / "missing-prompts"),
    )
    with pytest.raises(PromptNotFoundError, match="runtime_agent.llm_global_prompt_header"):
        prompt_builder.append_global_ai_instructions(
            "base prompt",
            {"custom_prompt": "stay focused"},
        )
