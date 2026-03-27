from __future__ import annotations

from pathlib import Path
import re
from typing import Any, Mapping

_TEMPLATE_PATTERN = re.compile(r"{{\s*(\w+)\s*}}")


class PromptNotFoundError(FileNotFoundError):
    """Raised when a prompt template cannot be located."""


def render_prompt_template(template: str, variables: Mapping[str, Any]) -> str:
    """Render ``{{ name }}`` placeholders with provided variables."""

    def replace(match: re.Match[str]) -> str:
        key = match.group(1)
        if key not in variables:
            raise KeyError(f"Missing template variable: {key}")
        return str(variables[key])

    return _TEMPLATE_PATTERN.sub(replace, template)


class PromptLoader:
    """Load prompt templates from shared resources and render them."""

    def __init__(self, root: Path | str | None = None) -> None:
        self.root = Path(root) if root is not None else Path(__file__).with_name("templates")

    def load(self, template_name: str, *, fallback_template: str | None = None) -> str:
        path = self._resolve(template_name)
        try:
            return path.read_text(encoding="utf-8-sig")
        except FileNotFoundError as exc:
            if fallback_template is not None:
                return fallback_template
            raise PromptNotFoundError(f"Prompt template not found: {template_name}") from exc

    def render(
        self,
        template_name: str,
        variables: Mapping[str, Any] | None = None,
        *,
        fallback_template: str | None = None,
    ) -> str:
        template = self.load(template_name, fallback_template=fallback_template)
        return render_prompt_template(template, variables or {})

    def _resolve(self, template_name: str) -> Path:
        candidate = (self.root / template_name).resolve()
        root = self.root.resolve()
        if not candidate.is_relative_to(root):
            raise ValueError(f"Template path escapes prompt root: {template_name}")
        return candidate
