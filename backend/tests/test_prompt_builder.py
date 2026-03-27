"""Tests for prompt builder resource loading."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from llm import prompt_builder


def test_append_global_ai_instructions_uses_resource_header():
    prompt = prompt_builder.append_global_ai_instructions(
        "base prompt",
        {"custom_prompt": "stay focused"},
    )

    expected_header = prompt_builder._GLOBAL_PROMPT_HEADER_RESOURCE_PATH.read_text(encoding="utf-8").strip()
    assert prompt == f"base prompt\n\n{expected_header}\nstay focused"


def test_append_global_ai_instructions_falls_back_when_resource_missing(monkeypatch):
    monkeypatch.setattr(
        prompt_builder,
        "_GLOBAL_PROMPT_HEADER_RESOURCE_PATH",
        Path(__file__).parent / "missing-global-header.txt",
    )

    prompt = prompt_builder.append_global_ai_instructions(
        "base prompt",
        {"custom_prompt": "stay focused"},
    )

    assert prompt == (
        f"base prompt\n\n{prompt_builder._GLOBAL_PROMPT_HEADER}\nstay focused"
    )
