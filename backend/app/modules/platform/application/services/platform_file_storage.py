from __future__ import annotations

from pathlib import Path, PurePosixPath

PLATFORM_FS_PROVIDER = "platform_fs"


def default_platform_storage_root() -> Path:
    return Path(__file__).resolve().parents[6] / "runtime" / "platform"


def normalize_object_key(*parts: object) -> str:
    cleaned: list[str] = []
    for part in parts:
        value = str(part or "").replace("\\", "/").strip("/")
        if not value:
            continue
        path = PurePosixPath(value)
        if path.is_absolute() or any(segment in {"", ".", ".."} for segment in path.parts):
            raise ValueError(f"platform storage object_key is invalid: {part}")
        cleaned.extend(path.parts)
    if not cleaned:
        raise ValueError("platform storage object_key is required")
    return str(PurePosixPath(*cleaned))


def resolve_platform_storage_path(object_key: str, *, storage_root: Path | None = None) -> Path:
    root = (storage_root or default_platform_storage_root()).resolve()
    normalized = normalize_object_key(object_key)
    path = (root / Path(normalized)).resolve()
    if path != root and root not in path.parents:
        raise ValueError(f"platform storage object_key escapes storage root: {object_key}")
    return path


def object_key_from_platform_path(path: Path, *, storage_root: Path | None = None) -> str:
    root = (storage_root or default_platform_storage_root()).resolve()
    resolved = path.resolve()
    try:
        relative = resolved.relative_to(root)
    except ValueError as error:
        raise ValueError(f"platform storage path is outside storage root: {path}") from error
    return normalize_object_key(relative.as_posix())


def try_object_key_from_platform_path(path: Path, *, storage_root: Path | None = None) -> str | None:
    try:
        return object_key_from_platform_path(path, storage_root=storage_root)
    except ValueError:
        return None
