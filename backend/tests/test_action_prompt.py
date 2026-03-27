"""Tests for the action prompt builder."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.prompting import PromptLoader
from approval import action_prompt as action_prompt_module
from approval.action_prompt import build_action_prompt


def test_action_prompt_includes_required_keywords():
    requirements = "描述用户对关键要素的要求。"
    prompt = build_action_prompt(requirements)

    assert "Output ONLY JSON" in prompt
    assert "actions" in prompt
    assert "User Input Requirements" in prompt
    assert requirements in prompt


def test_action_prompt_delivers_action_template():
    prompt = build_action_prompt("需要明确的步骤")

    assert '{"actions"' in prompt
    assert '"kind"' in prompt
    assert "title" in prompt
    assert '"reason"' in prompt
    assert "payload" in prompt


def test_action_prompt_template_exists_for_real_loader():
    loader = PromptLoader(root=action_prompt_module._PROMPT_ROOT)

    template = loader.load("action_prompt.txt")

    assert "{{ requirements_line }}" in template
    assert "Output ONLY JSON" in template
    assert '{"actions": [' in template


def test_action_prompt_uses_prompt_loader_with_trimmed_requirements(monkeypatch):
    class FakeLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-from-loader"

    loader = FakeLoader()
    monkeypatch.setattr(action_prompt_module, "_PROMPT_LOADER", loader)

    prompt = build_action_prompt("  需要明确的步骤  ")

    assert prompt == "rendered-from-loader"
    assert loader.calls == [
        (
            "action_prompt.txt",
            {"requirements_line": "需要明确的步骤"},
            action_prompt_module._ACTION_PROMPT_TEMPLATE,
        )
    ]


def test_action_prompt_uses_default_requirements_when_input_empty(monkeypatch):
    class FakeLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-default"

    loader = FakeLoader()
    monkeypatch.setattr(action_prompt_module, "_PROMPT_LOADER", loader)

    prompt = build_action_prompt("   ")

    assert prompt == "rendered-default"
    assert loader.calls[0][1] == {"requirements_line": "请提供必须满足的输入信息。"}
