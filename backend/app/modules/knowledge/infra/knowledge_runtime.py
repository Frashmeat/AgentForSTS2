from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import threading
import time
import uuid
import zipfile
from pathlib import Path
from typing import Any

import httpx

from app.shared.infra.config import settings as settings_module
from config import get_config

BACKEND_ROOT = Path(__file__).resolve().parents[4]
REPO_ROOT = BACKEND_ROOT.parent
# 仓库内静态知识文件只作为首次初始化 runtime/knowledge 的 seed source，
# 不是运行时并列真源。
GAME_REFERENCE_SEED_PATH = BACKEND_ROOT / "agents" / "sts2_api_reference.md"
KNOWLEDGE_ROOT = settings_module.RUNTIME_CONFIG_PATH.parent / "knowledge"
GAME_KNOWLEDGE_DIR = KNOWLEDGE_ROOT / "game"
BASELIB_KNOWLEDGE_DIR = KNOWLEDGE_ROOT / "baselib"
RESOURCE_KNOWLEDGE_DIR = KNOWLEDGE_ROOT / "resources" / "sts2"
KNOWLEDGE_CACHE_DIR = KNOWLEDGE_ROOT / "cache"
KNOWLEDGE_PACKS_DIR = KNOWLEDGE_ROOT / "packs"
KNOWLEDGE_MANIFEST_PATH = KNOWLEDGE_ROOT / "knowledge-manifest.json"
ACTIVE_KNOWLEDGE_PACK_PATH = KNOWLEDGE_ROOT / "active-knowledge-pack.json"
BASELIB_REFERENCE_SEED_PATH = BACKEND_ROOT / "agents" / "baselib_src" / "BaseLib.decompiled.cs"
GAME_KNOWLEDGE_SEED_DIR = BACKEND_ROOT / "agents" / "game_seed"
GAME_KNOWLEDGE_SEED_FILE = GAME_REFERENCE_SEED_PATH
BASELIB_KNOWLEDGE_SEED_FILE = BASELIB_REFERENCE_SEED_PATH
RESOURCE_KNOWLEDGE_SEED_DIR = BACKEND_ROOT / "app" / "modules" / "knowledge" / "resources" / "sts2"
STS2_DLL_RELATIVE = Path("data_sts2_windows_x86_64") / "sts2.dll"
BASELIB_RELEASES_URL = "https://github.com/Alchyr/BaseLib-StS2/releases"
BASELIB_RELEASES_API = "https://api.github.com/repos/Alchyr/BaseLib-StS2/releases/latest"

_REFRESH_TASKS: dict[str, _RefreshTask] = {}
_REFRESH_TASKS_LOCK = threading.Lock()
_MAX_RETAINED_REFRESH_TASKS = 10
_REFRESH_TERMINAL_STATUSES = frozenset({"completed", "failed", "cancelled"})


def _prune_refresh_tasks_locked() -> None:
    """剔除过量的已结束知识库刷新任务，避免 _REFRESH_TASKS 无界增长。调用方须持有 _REFRESH_TASKS_LOCK。"""
    terminal_ids = [task_id for task_id, task in _REFRESH_TASKS.items() if task.status in _REFRESH_TERMINAL_STATUSES]
    excess = len(terminal_ids) - _MAX_RETAINED_REFRESH_TASKS
    if excess > 0:
        for stale_id in terminal_ids[:excess]:
            _REFRESH_TASKS.pop(stale_id, None)


_ILSPY_PATH_ENV = "SPIREFORGE_ILSPYCMD_PATH"
_ILSPY_CANDIDATE_NAMES = ("ilspycmd.exe", "ilspycmd", "ILSpyCmd.dll", "ilspycmd.dll")
_ILSPY_SEARCH_ROOTS = (
    settings_module.RUNTIME_CONFIG_PATH.parent / "tools",
    REPO_ROOT,
    REPO_ROOT / "tools",
    REPO_ROOT / "runtime",
    REPO_ROOT / "tools" / "latest" / "artifacts",
)
_ILSPY_SKIP_DIRS = {".git", ".venv", "node_modules", "__pycache__", ".tmp"}


def ensure_knowledge_dirs() -> None:
    KNOWLEDGE_ROOT.mkdir(parents=True, exist_ok=True)
    KNOWLEDGE_CACHE_DIR.mkdir(parents=True, exist_ok=True)
    KNOWLEDGE_PACKS_DIR.mkdir(parents=True, exist_ok=True)
    GAME_KNOWLEDGE_DIR.mkdir(parents=True, exist_ok=True)
    BASELIB_KNOWLEDGE_DIR.mkdir(parents=True, exist_ok=True)
    RESOURCE_KNOWLEDGE_DIR.mkdir(parents=True, exist_ok=True)


def _copy_missing_tree(source_dir: Path, target_dir: Path) -> None:
    if not source_dir.exists():
        return

    for item in source_dir.rglob("*"):
        relative_path = item.relative_to(source_dir)
        destination = target_dir / relative_path
        if item.is_dir():
            destination.mkdir(parents=True, exist_ok=True)
            continue
        if destination.exists():
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item, destination)


def _copy_missing_file(source_file: Path, target_file: Path) -> None:
    if not source_file.exists() or target_file.exists():
        return
    target_file.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source_file, target_file)


def ensure_runtime_knowledge_seeded() -> None:
    ensure_knowledge_dirs()
    _copy_missing_tree(Path(GAME_KNOWLEDGE_SEED_DIR), GAME_KNOWLEDGE_DIR)
    _copy_missing_file(Path(GAME_KNOWLEDGE_SEED_FILE), GAME_KNOWLEDGE_DIR / Path(GAME_KNOWLEDGE_SEED_FILE).name)
    _copy_missing_file(Path(BASELIB_KNOWLEDGE_SEED_FILE), BASELIB_KNOWLEDGE_DIR / "BaseLib.decompiled.cs")
    _copy_missing_tree(Path(RESOURCE_KNOWLEDGE_SEED_DIR), RESOURCE_KNOWLEDGE_DIR)


def _walk_candidate_paths(root: Path) -> list[Path]:
    candidates: list[Path] = []
    if not root.exists():
        return candidates

    for current_root, dirnames, filenames in os.walk(root):
        dirnames[:] = [name for name in dirnames if name not in _ILSPY_SKIP_DIRS]
        for filename in filenames:
            if filename in _ILSPY_CANDIDATE_NAMES:
                candidates.append(Path(current_root) / filename)
    return candidates


def _command_for_ilspy_path(path: Path) -> list[str] | None:
    suffix = path.suffix.lower()
    if suffix in {".exe", ""}:
        return [str(path)]
    if suffix == ".dll":
        dotnet_path = shutil.which("dotnet")
        if dotnet_path:
            return [dotnet_path, str(path)]
    return None


def resolve_ilspycmd_command() -> list[str] | None:
    explicit_path = str(os.environ.get(_ILSPY_PATH_ENV, "")).strip()
    if explicit_path:
        candidate = Path(explicit_path)
        if candidate.exists():
            return _command_for_ilspy_path(candidate)

    seen: set[Path] = set()
    for root in _ILSPY_SEARCH_ROOTS:
        for candidate in _walk_candidate_paths(root):
            resolved = candidate.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            command = _command_for_ilspy_path(resolved)
            if command is not None:
                return command

    path_hit = shutil.which("ilspycmd")
    if path_hit:
        return [path_hit]
    return None


def _has_ilspycmd() -> bool:
    return resolve_ilspycmd_command() is not None


def load_manifest() -> dict[str, Any] | None:
    if not KNOWLEDGE_MANIFEST_PATH.exists():
        return None
    with open(KNOWLEDGE_MANIFEST_PATH, encoding="utf-8") as file:
        return json.load(file)


def save_manifest(payload: dict[str, Any]) -> None:
    ensure_knowledge_dirs()
    tmp_path = KNOWLEDGE_MANIFEST_PATH.with_suffix(".tmp")
    with open(tmp_path, "w", encoding="utf-8") as file:
        json.dump(payload, file, indent=2, ensure_ascii=False)
    tmp_path.replace(KNOWLEDGE_MANIFEST_PATH)


def _now_text() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S%z")


def _pack_meta_path(pack_id: str) -> Path:
    return KNOWLEDGE_PACKS_DIR / pack_id / "pack-meta.json"


def _pack_content_dir(pack_id: str) -> Path:
    return KNOWLEDGE_PACKS_DIR / pack_id / "content"


def _safe_pack_id(value: str) -> str:
    text = str(value or "").strip()
    if not text or any(
        char not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-" for char in text
    ):
        raise ValueError("invalid knowledge pack id")
    return text


def _read_json_file(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    with open(path, encoding="utf-8") as file:
        payload = json.load(file)
    return payload if isinstance(payload, dict) else None


def get_active_knowledge_pack() -> dict[str, Any] | None:
    active = _read_json_file(ACTIVE_KNOWLEDGE_PACK_PATH)
    if not active:
        return None
    pack_id = str(active.get("pack_id", "")).strip()
    if not pack_id:
        return None
    meta = _read_json_file(_pack_meta_path(pack_id))
    if meta is None:
        return None
    return {
        **meta,
        "active": True,
        "activated_at": active.get("activated_at"),
        "previous_pack_id": active.get("previous_pack_id", ""),
    }


def _active_content_dir() -> Path | None:
    active = get_active_knowledge_pack()
    if not active:
        return None
    content_dir = _pack_content_dir(str(active["pack_id"]))
    return content_dir if content_dir.exists() else None


def active_resource_knowledge_dir() -> Path:
    content_dir = _active_content_dir()
    if content_dir is not None and (content_dir / "resources" / "sts2").exists():
        return content_dir / "resources" / "sts2"
    return RESOURCE_KNOWLEDGE_DIR


def active_game_knowledge_dir() -> Path:
    content_dir = _active_content_dir()
    if content_dir is not None and (content_dir / "game").exists():
        return content_dir / "game"
    return GAME_KNOWLEDGE_DIR


def active_baselib_knowledge_dir() -> Path:
    content_dir = _active_content_dir()
    if content_dir is not None and (content_dir / "baselib").exists():
        return content_dir / "baselib"
    return BASELIB_KNOWLEDGE_DIR


def list_knowledge_packs() -> dict[str, Any]:
    ensure_knowledge_dirs()
    active = get_active_knowledge_pack()
    active_pack_id = str(active.get("pack_id", "")) if active else ""
    items: list[dict[str, Any]] = []
    for meta_path in sorted(KNOWLEDGE_PACKS_DIR.glob("*/pack-meta.json")):
        meta = _read_json_file(meta_path)
        if meta is None:
            continue
        pack_id = str(meta.get("pack_id", "")).strip()
        items.append({**meta, "active": pack_id == active_pack_id})
    return {"active_pack_id": active_pack_id, "active_pack": active, "items": items}


def _validate_zip_member(member_name: str) -> Path:
    normalized = Path(member_name)
    if normalized.is_absolute() or ".." in normalized.parts:
        raise ValueError(f"unsafe knowledge pack path: {member_name}")
    return normalized


def _knowledge_pack_file_stats(files: list[str]) -> dict[str, int]:
    return {
        "resource_md_count": sum(
            1 for path in files if path.startswith("resources/sts2/") and path.lower().endswith(".md")
        ),
        "game_cs_count": sum(1 for path in files if path.startswith("game/") and path.lower().endswith(".cs")),
        "baselib_cs_count": sum(1 for path in files if path.startswith("baselib/") and path.lower().endswith(".cs")),
    }


def upload_knowledge_pack_zip(content: bytes, *, file_name: str = "", label: str = "") -> dict[str, Any]:
    ensure_knowledge_dirs()
    pack_id = uuid.uuid4().hex
    pack_root = KNOWLEDGE_PACKS_DIR / pack_id
    content_dir = _pack_content_dir(pack_id)
    content_dir.mkdir(parents=True, exist_ok=False)
    zip_path = pack_root / (Path(file_name).name or "knowledge-pack.zip")
    zip_path.write_bytes(content)

    extracted_files: list[str] = []
    try:
        with zipfile.ZipFile(zip_path) as archive:
            for member in archive.infolist():
                relative = _validate_zip_member(member.filename)
                if member.is_dir():
                    continue
                target = content_dir / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(member) as source, open(target, "wb") as destination:
                    shutil.copyfileobj(source, destination)
                extracted_files.append(str(relative).replace("\\", "/"))
    except Exception:
        shutil.rmtree(pack_root, ignore_errors=True)
        raise

    file_list = sorted(dict.fromkeys(extracted_files))
    file_stats = _knowledge_pack_file_stats(file_list)
    meta = {
        "pack_id": pack_id,
        "label": label.strip() or Path(file_name).stem or pack_id,
        "file_name": Path(file_name).name,
        "created_at": _now_text(),
        "storage_path": str(pack_root),
        "content_path": str(content_dir),
        "file_count": len(file_list),
        "files": file_list,
        **file_stats,
        "has_resources": (content_dir / "resources" / "sts2").exists(),
        "has_game": (content_dir / "game").exists(),
        "has_baselib": (content_dir / "baselib" / "BaseLib.decompiled.cs").exists(),
    }
    with open(_pack_meta_path(pack_id), "w", encoding="utf-8") as file:
        json.dump(meta, file, indent=2, ensure_ascii=False)
    return meta


def _write_knowledge_tree_to_zip(
    archive: zipfile.ZipFile,
    *,
    source_dir: Path,
    archive_root: Path,
) -> list[str]:
    if not source_dir.exists():
        return []

    archived_files: list[str] = []
    for path in sorted(item for item in source_dir.rglob("*") if item.is_file()):
        relative = path.relative_to(source_dir)
        archive_name = str(archive_root / relative).replace("\\", "/")
        archive.write(path, archive_name)
        archived_files.append(archive_name)
    return archived_files


def export_current_knowledge_pack_zip() -> dict[str, Any]:
    ensure_runtime_knowledge_seeded()
    buffer = io.BytesIO()
    files: list[str] = []
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        files.extend(
            _write_knowledge_tree_to_zip(
                archive,
                source_dir=active_resource_knowledge_dir(),
                archive_root=Path("resources") / "sts2",
            )
        )
        files.extend(
            _write_knowledge_tree_to_zip(
                archive,
                source_dir=active_game_knowledge_dir(),
                archive_root=Path("game"),
            )
        )
        files.extend(
            _write_knowledge_tree_to_zip(
                archive,
                source_dir=active_baselib_knowledge_dir(),
                archive_root=Path("baselib"),
            )
        )

    file_list = sorted(dict.fromkeys(files))
    if not file_list:
        raise ValueError("current knowledge pack has no files to export")
    file_stats = _knowledge_pack_file_stats(file_list)
    return {
        "content": buffer.getvalue(),
        "file_name": "workstation-current-knowledge-pack.zip",
        "file_count": len(file_list),
        "files": file_list,
        **file_stats,
    }


def activate_knowledge_pack(pack_id: str) -> dict[str, Any]:
    safe_pack_id = _safe_pack_id(pack_id)
    meta = _read_json_file(_pack_meta_path(safe_pack_id))
    if meta is None:
        raise KeyError(safe_pack_id)
    active = _read_json_file(ACTIVE_KNOWLEDGE_PACK_PATH) or {}
    previous_pack_id = str(active.get("pack_id", "")).strip()
    payload = {
        "pack_id": safe_pack_id,
        "activated_at": _now_text(),
        "previous_pack_id": (
            previous_pack_id if previous_pack_id != safe_pack_id else str(active.get("previous_pack_id", ""))
        ),
    }
    ACTIVE_KNOWLEDGE_PACK_PATH.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = ACTIVE_KNOWLEDGE_PACK_PATH.with_suffix(".tmp")
    with open(tmp_path, "w", encoding="utf-8") as file:
        json.dump(payload, file, indent=2, ensure_ascii=False)
    tmp_path.replace(ACTIVE_KNOWLEDGE_PACK_PATH)
    return get_active_knowledge_pack() or {**meta, "active": True}


def rollback_knowledge_pack() -> dict[str, Any]:
    active = _read_json_file(ACTIVE_KNOWLEDGE_PACK_PATH) or {}
    previous_pack_id = str(active.get("previous_pack_id", "")).strip()
    if previous_pack_id:
        return activate_knowledge_pack(previous_pack_id)
    if ACTIVE_KNOWLEDGE_PACK_PATH.exists():
        ACTIVE_KNOWLEDGE_PACK_PATH.unlink()
    return {"active_pack_id": "", "active_pack": None}


def _manifest_game_dir(manifest: dict[str, Any] | None) -> str:
    if manifest:
        return str(manifest.get("game", {}).get("sts2_path", "")).strip()
    return str(get_config().get("sts2_path", "")).strip()


def _manifest_knowledge_path(manifest: dict[str, Any], section: str, default_path: Path) -> str:
    section_payload = manifest.get(section, {})
    return str(section_payload.get("knowledge_path") or section_payload.get("decompiled_src_path") or default_path)


def _default_status_payload(status: str) -> dict[str, Any]:
    return {
        "status": status,
        "generated_at": None,
        "checked_at": None,
        "warnings": [],
        "game": {
            "configured_path": str(get_config().get("sts2_path", "")).strip(),
            "version": None,
            "current_version": None,
            "matches": None,
            "version_source": "steam_app_manifest",
            "source_mode": _resolve_game_source_mode(),
            "knowledge_path": str(active_game_knowledge_dir()),
            "decompiled_src_path": str(active_game_knowledge_dir()),
        },
        "baselib": {
            "release_tag": None,
            "latest_release_tag": None,
            "matches": None,
            "release_url": BASELIB_RELEASES_URL,
            "source_mode": _resolve_baselib_source_mode(),
            "knowledge_path": str(active_baselib_knowledge_dir()),
            "decompiled_src_path": str(active_baselib_knowledge_dir()),
        },
    }


def _parse_acf(content: str) -> dict[str, str]:
    parsed: dict[str, str] = {}
    for line in content.splitlines():
        parts = [segment.strip() for segment in line.strip().split('"') if segment.strip()]
        if len(parts) >= 2:
            parsed[parts[0]] = parts[1]
    return parsed


def read_current_game_version(game_path: str) -> dict[str, str]:
    game_root = Path(str(game_path or "").strip())
    if not game_root.exists():
        raise FileNotFoundError(f"STS2 path not found: {game_root}")
    library_root = game_root.parent.parent
    target_dir = game_root.name.lower()
    for manifest_path in sorted(library_root.glob("appmanifest_*.acf")):
        data = _parse_acf(manifest_path.read_text(encoding="utf-8", errors="replace"))
        install_dir = str(data.get("installdir", "")).strip().lower()
        if install_dir != target_dir:
            continue
        version = str(data.get("buildid") or data.get("LastUpdated") or "").strip()
        if version:
            return {
                "version": version,
                "source": "steam_app_manifest",
                "manifest_path": str(manifest_path),
            }
    raise FileNotFoundError(f"Steam app manifest not found for {game_root}")


def fetch_latest_baselib_release() -> dict[str, Any]:
    response = httpx.get(
        BASELIB_RELEASES_API,
        headers={"User-Agent": "AgentTheSpire-Codex"},
        timeout=15.0,
    )
    response.raise_for_status()
    return response.json()


def select_baselib_asset(release: dict[str, Any]) -> dict[str, Any]:
    assets = list(release.get("assets", []))
    for asset in assets:
        if str(asset.get("name", "")).strip().lower() == "baselib.dll":
            return asset
    for asset in assets:
        name = str(asset.get("name", "")).strip().lower()
        if name.endswith(".dll") and "baselib" in name:
            return asset

    release_tag = str(release.get("tag_name", "")).strip() or "unknown"
    asset_names = [str(asset.get("name", "")).strip() for asset in assets if str(asset.get("name", "")).strip()]
    asset_names_text = ", ".join(asset_names) if asset_names else "无资产"
    raise RuntimeError(
        "BaseLib latest release 未找到可下载的 DLL 资产。"
        f" release={release_tag}。"
        " 当前只接受 .dll；优先 BaseLib.dll，其次文件名包含 baselib 且后缀为 .dll。"
        f" 本次 release 资产列表：{asset_names_text}"
    )


def _directory_has_sources(path: Path) -> bool:
    return path.exists() and any(path.rglob("*.cs"))


def _resolve_game_source_mode() -> str:
    if _directory_has_sources(active_game_knowledge_dir()):
        return "runtime_decompiled"
    return "missing"


def _resolve_baselib_source_mode() -> str:
    if (active_baselib_knowledge_dir() / "BaseLib.decompiled.cs").exists():
        return "runtime_decompiled"
    return "missing"


def get_knowledge_status() -> dict[str, Any]:
    manifest = load_manifest()
    if manifest is None:
        payload = _default_status_payload("missing")
        payload["warnings"].append("知识库 manifest 不存在")
        configured_path = str(get_config().get("sts2_path", "")).strip()
        if not configured_path:
            payload["warnings"].append("未配置 STS2 游戏路径，无法更新知识库")
        if not _has_ilspycmd():
            payload["warnings"].append("未检测到 ilspycmd，无法反编译游戏和 BaseLib（会先查项目目录，再查 PATH）")
        if not _directory_has_sources(active_game_knowledge_dir()):
            payload["warnings"].append("游戏反编译源码目录为空，请先执行“更新知识库”")
        if not (active_baselib_knowledge_dir() / "BaseLib.decompiled.cs").exists():
            payload["warnings"].append("BaseLib 反编译结果缺失，请先执行“更新知识库”")
        return payload

    payload = _default_status_payload("fresh")
    payload["generated_at"] = manifest.get("generated_at")
    payload["checked_at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    payload["game"]["configured_path"] = _manifest_game_dir(manifest)
    payload["game"]["version"] = manifest.get("game", {}).get("version")
    payload["baselib"]["release_tag"] = manifest.get("baselib", {}).get("release_tag")
    payload["game"]["source_mode"] = _resolve_game_source_mode()
    payload["baselib"]["source_mode"] = _resolve_baselib_source_mode()
    payload["game"]["knowledge_path"] = _manifest_knowledge_path(manifest, "game", active_game_knowledge_dir())
    payload["baselib"]["knowledge_path"] = _manifest_knowledge_path(manifest, "baselib", active_baselib_knowledge_dir())
    payload["game"]["decompiled_src_path"] = str(
        manifest.get("game", {}).get("decompiled_src_path", payload["game"]["knowledge_path"])
    )
    payload["baselib"]["decompiled_src_path"] = str(
        manifest.get("baselib", {}).get("decompiled_src_path", payload["baselib"]["knowledge_path"])
    )

    game_path = _manifest_game_dir(manifest)
    try:
        game_info = read_current_game_version(game_path)
        payload["game"]["current_version"] = game_info["version"]
        payload["game"]["version_source"] = game_info["source"]
        payload["game"]["matches"] = payload["game"]["version"] == game_info["version"]
    except Exception as exc:
        payload["warnings"].append(str(exc))
        payload["game"]["matches"] = None

    try:
        release = fetch_latest_baselib_release()
        payload["baselib"]["latest_release_tag"] = release.get("tag_name")
        payload["baselib"]["matches"] = payload["baselib"]["release_tag"] == release.get("tag_name")
    except Exception as exc:
        payload["warnings"].append(str(exc))
        payload["baselib"]["matches"] = None

    game_runtime_ready = _directory_has_sources(Path(payload["game"]["knowledge_path"]))
    baselib_runtime_ready = Path(str(payload["baselib"]["knowledge_path"])).joinpath("BaseLib.decompiled.cs").exists()
    if not game_runtime_ready or not baselib_runtime_ready:
        payload["status"] = "missing"
        return payload

    if payload["game"]["matches"] is False or payload["baselib"]["matches"] is False:
        payload["status"] = "stale"
        return payload

    if payload["warnings"]:
        payload["status"] = "stale"
        return payload

    payload["status"] = "fresh"
    return payload


def check_knowledge_status() -> dict[str, Any]:
    return get_knowledge_status()


def _reset_dir(path: Path) -> None:
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True, exist_ok=True)


def _find_game_dll(sts2_path: str) -> Path:
    game_dll = Path(sts2_path) / STS2_DLL_RELATIVE
    if not game_dll.exists():
        raise FileNotFoundError(f"sts2.dll not found at {game_dll}")
    return game_dll


def _run_ilspy_outputdir(dll_path: Path, output_dir: Path) -> None:
    _reset_dir(output_dir)
    ilspy_command = resolve_ilspycmd_command()
    if ilspy_command is None:
        raise RuntimeError("未检测到 ilspycmd，请先放到项目目录或确保 ilspycmd 在 PATH 中")
    result = subprocess.run(
        [*ilspy_command, str(dll_path), "--outputdir", str(output_dir)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout or "ilspycmd failed").strip())


def _run_ilspy_to_single_file(dll_path: Path, output_file: Path) -> None:
    output_file.parent.mkdir(parents=True, exist_ok=True)
    ilspy_command = resolve_ilspycmd_command()
    if ilspy_command is None:
        raise RuntimeError("未检测到 ilspycmd，请先放到项目目录或确保 ilspycmd 在 PATH 中")
    result = subprocess.run(
        [*ilspy_command, str(dll_path)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout or "ilspycmd failed").strip())
    output_file.write_text(result.stdout, encoding="utf-8")


def _download_file(url: str, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    with httpx.stream(
        "GET",
        url,
        headers={"User-Agent": "AgentTheSpire-Codex"},
        timeout=60.0,
        follow_redirects=True,
    ) as response:
        response.raise_for_status()
        with open(target, "wb") as file:
            for chunk in response.iter_bytes():
                file.write(chunk)


class _RefreshTask:
    def __init__(self, task_id: str):
        self.task_id = task_id
        self.status = "pending"
        self.current_step = "等待开始"
        self.notes: list[str] = ["开始更新知识库"]
        self.error: str | None = None
        self.can_cancel = False
        self._lock = threading.Lock()
        self._thread = threading.Thread(target=self._run, name=f"knowledge-refresh-{task_id}", daemon=True)

    def start(self) -> None:
        with self._lock:
            self.status = "running"
            self.current_step = "初始化更新任务"
        self._thread.start()

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            return {
                "task_id": self.task_id,
                "status": self.status,
                "current_step": self.current_step,
                "notes": list(self.notes),
                "error": self.error,
                "can_cancel": self.can_cancel,
            }

    def set_step(self, step: str) -> None:
        with self._lock:
            self.current_step = step
            if not self.notes or self.notes[-1] != step:
                self.notes.append(step)

    def fail(self, error: str) -> None:
        with self._lock:
            self.status = "failed"
            self.current_step = "更新失败"
            self.error = error
            self.notes.append(f"更新失败：{error}")

    def finish(self) -> None:
        with self._lock:
            self.status = "completed"
            self.current_step = "更新完成"
            self.notes.append("更新完成")

    def _run(self) -> None:
        try:
            _run_refresh_impl(self)
            self.finish()
        except Exception as exc:
            self.fail(str(exc))


def _run_refresh_impl(task: _RefreshTask) -> None:
    ensure_knowledge_dirs()
    sts2_path = _validate_refresh_prereqs(get_config())

    task.set_step("读取当前游戏版本")
    game_info = read_current_game_version(sts2_path)

    task.set_step("反编译游戏源码")
    _decompile_game_dll(sts2_path)

    task.set_step("读取 Baselib latest release")
    release = fetch_latest_baselib_release()
    asset = select_baselib_asset(release)

    task.set_step("下载 Baselib.dll")
    baselib_dll_path = _download_baselib(asset)

    task.set_step("反编译 Baselib")
    _decompile_baselib(baselib_dll_path)

    save_manifest(
        _build_refresh_manifest(
            sts2_path=sts2_path,
            game_info=game_info,
            release=release,
            asset=asset,
            baselib_dll_path=baselib_dll_path,
        )
    )


def _validate_refresh_prereqs(config: dict[str, Any]) -> str:
    sts2_path = str(config.get("sts2_path", "")).strip()
    if not sts2_path:
        raise RuntimeError("未配置 STS2 游戏路径，无法更新知识库")
    if not _has_ilspycmd():
        raise RuntimeError("未检测到 ilspycmd，请先放到项目目录或确保 ilspycmd 在 PATH 中")
    return sts2_path


def _decompile_game_dll(sts2_path: str) -> None:
    _run_ilspy_outputdir(_find_game_dll(sts2_path), GAME_KNOWLEDGE_DIR)


def _download_baselib(asset: dict[str, Any]) -> Path:
    target = KNOWLEDGE_CACHE_DIR / str(asset["name"]).strip()
    _download_file(str(asset["browser_download_url"]).strip(), target)
    return target


def _decompile_baselib(baselib_dll_path: Path) -> None:
    _reset_dir(BASELIB_KNOWLEDGE_DIR)
    _run_ilspy_to_single_file(baselib_dll_path, BASELIB_KNOWLEDGE_DIR / "BaseLib.decompiled.cs")


def _build_refresh_manifest(
    *,
    sts2_path: str,
    game_info: dict[str, Any],
    release: dict[str, Any],
    asset: dict[str, Any],
    baselib_dll_path: Path,
) -> dict[str, Any]:
    timestamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    return {
        "schema_version": 1,
        "status": "fresh",
        "generated_at": timestamp,
        "game": {
            "sts2_path": sts2_path,
            "version": game_info["version"],
            "version_source": game_info["source"],
            "knowledge_path": str(active_game_knowledge_dir()),
            "decompiled_src_path": str(active_game_knowledge_dir()),
        },
        "baselib": {
            "release_tag": release.get("tag_name"),
            "release_published_at": release.get("published_at"),
            "asset_name": asset.get("name"),
            "downloaded_file_path": str(baselib_dll_path),
            "knowledge_path": str(active_baselib_knowledge_dir()),
            "decompiled_src_path": str(active_baselib_knowledge_dir()),
        },
        "last_check": {
            "checked_at": timestamp,
            "game_matches": True,
            "baselib_matches": True,
            "warnings": [],
        },
    }


def start_refresh_task() -> dict[str, Any]:
    task_id = uuid.uuid4().hex
    task = _RefreshTask(task_id)
    with _REFRESH_TASKS_LOCK:
        _REFRESH_TASKS[task_id] = task
        _prune_refresh_tasks_locked()
    task.start()
    time.sleep(0.01)
    return task.snapshot()


def get_refresh_task(task_id: str) -> dict[str, Any]:
    with _REFRESH_TASKS_LOCK:
        task = _REFRESH_TASKS.get(task_id)
    if task is None:
        raise KeyError(task_id)
    return task.snapshot()


def get_latest_refresh_task() -> dict[str, Any] | None:
    with _REFRESH_TASKS_LOCK:
        task = next(reversed(_REFRESH_TASKS.values()), None)
    return task.snapshot() if task is not None else None
