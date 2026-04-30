from __future__ import annotations

import re
from collections.abc import Mapping
from pathlib import Path
from typing import Any

_TEMPLATE_PATTERN = re.compile(r"{{\s*(\w+)\s*}}")
_BUNDLE_KEY_PATTERN = re.compile(r"^[a-z0-9_]+$")
_BUNDLE_HEADING_PREFIX = "## "
_FENCE_PATTERN = re.compile(r"^\s*(`{3,}|~{3,})")


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
        self.root = Path(root) if root is not None else Path(__file__).resolve().parent.parent / "resources" / "prompts"
        self._bundle_cache: dict[Path, tuple[int, int, dict[str, str]]] = {}

    def load(self, template_name: str, *, fallback_template: str | None = None) -> str:
        try:
            return self._load_template(template_name)
        except PromptNotFoundError:
            raise
        except FileNotFoundError as exc:
            if self._parse_bundle_request(template_name) is not None:
                raise PromptNotFoundError(f"Prompt template not found: {template_name}") from exc
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

    def _load_template(self, template_name: str) -> str:
        bundle_request = self._parse_bundle_request(template_name)
        if bundle_request is not None:
            bundle_name, key = bundle_request
            bundle_path = self._resolve_path(f"{bundle_name}.md")
            sections = self._load_bundle_sections(bundle_path)
            try:
                return sections[key]
            except KeyError as exc:
                raise PromptNotFoundError(f"Prompt template not found: {template_name}") from exc

        return self._resolve_path(template_name).read_text(encoding="utf-8-sig")

    def _parse_bundle_request(self, template_name: str) -> tuple[str, str] | None:
        if "/" in template_name or "\\" in template_name:
            return None
        if template_name.endswith(".md"):
            return None
        if template_name.count(".") != 1:
            return None

        bundle_name, key = template_name.split(".", 1)
        if not bundle_name or not key:
            return None

        if not _BUNDLE_KEY_PATTERN.fullmatch(bundle_name):
            return None
        if not _BUNDLE_KEY_PATTERN.fullmatch(key):
            return None

        bundle_path = self._resolve_path(f"{bundle_name}.md")
        if not bundle_path.is_file() and "_" not in key:
            return None
        return bundle_name, key

    def _parse_bundle_sections(self, content: str) -> dict[str, str]:
        sections: dict[str, str] = {}
        current_key: str | None = None
        current_lines: list[str] = []
        in_fence = False
        active_fence: str | None = None

        for line in content.splitlines(keepends=True):
            fence_match = _FENCE_PATTERN.match(line)
            if fence_match is not None:
                fence = fence_match.group(1)
                if in_fence and active_fence is not None and fence.startswith(active_fence[0]):
                    in_fence = False
                    active_fence = None
                elif not in_fence:
                    in_fence = True
                    active_fence = fence

            heading_key = None if in_fence else self._parse_bundle_heading(line)
            if heading_key is None:
                if current_key is not None:
                    current_lines.append(line)
                continue

            if heading_key == current_key or heading_key in sections:
                raise ValueError(f"Duplicate prompt bundle key: {heading_key}")

            if current_key is not None:
                sections[current_key] = self._finalize_section(current_key, current_lines)

            current_key = heading_key
            current_lines = []

        if current_key is not None:
            sections[current_key] = self._finalize_section(current_key, current_lines)

        return sections

    def _parse_bundle_heading(self, line: str) -> str | None:
        if not line.startswith(_BUNDLE_HEADING_PREFIX):
            return None

        key = line[len(_BUNDLE_HEADING_PREFIX) :].strip()
        if not _BUNDLE_KEY_PATTERN.fullmatch(key):
            raise ValueError(f"Invalid prompt bundle key: {key}")
        return key

    def _finalize_section(self, key: str, lines: list[str]) -> str:
        content = "".join(lines)
        if not content.strip():
            raise ValueError(f"Empty prompt bundle section: {key}")
        return content

    def _load_bundle_sections(self, bundle_path: Path) -> dict[str, str]:
        stat = bundle_path.stat()
        cached = self._bundle_cache.get(bundle_path)
        if cached is not None:
            cached_mtime_ns, cached_size, sections = cached
            if cached_mtime_ns == stat.st_mtime_ns and cached_size == stat.st_size:
                return sections

        sections = self._parse_bundle_sections(bundle_path.read_text(encoding="utf-8-sig"))
        self._bundle_cache[bundle_path] = (stat.st_mtime_ns, stat.st_size, sections)
        return sections

    def _resolve_path(self, template_name: str) -> Path:
        candidate = (self.root / template_name).resolve()
        root = self.root.resolve()
        if not candidate.is_relative_to(root):
            raise ValueError(f"Template path escapes prompt root: {template_name}")
        return candidate
