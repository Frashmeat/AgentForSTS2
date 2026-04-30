from __future__ import annotations

from pathlib import Path

_SKIP_DIRS = {"bin", "obj", ".godot", ".git"}
_MAX_SOURCE_SAMPLES = 8
_MAX_LOCALIZATION_SAMPLES = 4


def render_server_workspace_snapshot(workspace_root: object) -> str:
    root_text = str(workspace_root or "").strip()
    if not root_text:
        return "无"

    root = Path(root_text)
    if not root.exists():
        return f"工作区不存在：{root}"
    if not root.is_dir():
        return f"工作区不是目录：{root}"

    try:
        csproj_files = sorted(root.glob("*.csproj"))
        entry_files = [path for path in (root / "MainFile.cs", root / "ModEntry.cs") if path.exists()]
        source_files = _collect_samples(root=root, suffix=".cs", limit=_MAX_SOURCE_SAMPLES)
        localization_files = _collect_samples(
            root=root,
            suffix=".json",
            limit=_MAX_LOCALIZATION_SAMPLES,
            required_part="localization",
        )
    except OSError as exc:
        return f"工作区扫描失败：{exc}"

    lines = [f"项目根目录：{root}"]
    lines.append(_render_section("项目文件", csproj_files))
    lines.append(_render_section("入口文件", entry_files))
    lines.append(_render_section("源码样本", source_files))
    lines.append(_render_section("本地化样本", localization_files))
    return "\n".join(lines)


def _collect_samples(
    *,
    root: Path,
    suffix: str,
    limit: int,
    required_part: str | None = None,
) -> list[Path]:
    results: list[Path] = []
    for path in sorted(root.rglob(f"*{suffix}")):
        try:
            relative = path.relative_to(root)
        except ValueError:
            continue
        if any(part in _SKIP_DIRS for part in relative.parts):
            continue
        if required_part is not None and required_part not in relative.parts:
            continue
        results.append(relative)
        if len(results) >= limit:
            break
    return results


def _render_section(title: str, paths: list[Path]) -> str:
    if not paths:
        return f"{title}：无"
    return f"{title}：\n" + "\n".join(f"- {path.as_posix()}" for path in paths)
