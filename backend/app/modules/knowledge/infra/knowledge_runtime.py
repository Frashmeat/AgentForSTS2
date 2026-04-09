from __future__ import annotations

import json
import os
import shutil
import subprocess
import threading
import time
import uuid
from pathlib import Path
from typing import Any

import httpx

from app.shared.infra.config import settings as settings_module
from agents.sts2_docs import API_REF_PATH
from config import get_config, get_decompiled_src_path

BACKEND_ROOT = Path(__file__).resolve().parents[4]
REPO_ROOT = BACKEND_ROOT.parent
KNOWLEDGE_ROOT = settings_module.RUNTIME_CONFIG_PATH.parent / "knowledge"
GAME_DECOMPILED_DIR = KNOWLEDGE_ROOT / "game_decompiled"
BASELIB_DECOMPILED_DIR = KNOWLEDGE_ROOT / "baselib_decompiled"
KNOWLEDGE_CACHE_DIR = KNOWLEDGE_ROOT / "cache"
KNOWLEDGE_MANIFEST_PATH = KNOWLEDGE_ROOT / "knowledge-manifest.json"
BASELIB_FALLBACK_PATH = BACKEND_ROOT / "agents" / "baselib_src" / "BaseLib.decompiled.cs"
STS2_DLL_RELATIVE = Path("data_sts2_windows_x86_64") / "sts2.dll"
BASELIB_RELEASES_URL = "https://github.com/Alchyr/BaseLib-StS2/releases"
BASELIB_RELEASES_API = "https://api.github.com/repos/Alchyr/BaseLib-StS2/releases/latest"

_REFRESH_TASKS: dict[str, "_RefreshTask"] = {}
_REFRESH_TASKS_LOCK = threading.Lock()
_ILSPY_PATH_ENV = "SPIREFORGE_ILSPYCMD_PATH"
_ILSPY_CANDIDATE_NAMES = ("ilspycmd.exe", "ilspycmd", "ILSpyCmd.dll", "ilspycmd.dll")
_ILSPY_SEARCH_ROOTS = (
    REPO_ROOT,
    REPO_ROOT / "tools",
    REPO_ROOT / "runtime",
    REPO_ROOT / "tools" / "latest" / "artifacts",
)
_ILSPY_SKIP_DIRS = {".git", ".venv", "node_modules", "__pycache__", ".tmp"}


def ensure_knowledge_dirs() -> None:
    KNOWLEDGE_ROOT.mkdir(parents=True, exist_ok=True)
    KNOWLEDGE_CACHE_DIR.mkdir(parents=True, exist_ok=True)
    GAME_DECOMPILED_DIR.mkdir(parents=True, exist_ok=True)
    BASELIB_DECOMPILED_DIR.mkdir(parents=True, exist_ok=True)


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
    with open(KNOWLEDGE_MANIFEST_PATH, "r", encoding="utf-8") as file:
        return json.load(file)


def save_manifest(payload: dict[str, Any]) -> None:
    ensure_knowledge_dirs()
    tmp_path = KNOWLEDGE_MANIFEST_PATH.with_suffix(".tmp")
    with open(tmp_path, "w", encoding="utf-8") as file:
        json.dump(payload, file, indent=2, ensure_ascii=False)
    tmp_path.replace(KNOWLEDGE_MANIFEST_PATH)


def _manifest_game_dir(manifest: dict[str, Any] | None) -> str:
    if manifest:
        return str(manifest.get("game", {}).get("sts2_path", "")).strip()
    return str(get_config().get("sts2_path", "")).strip()


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
            "decompiled_src_path": str(GAME_DECOMPILED_DIR),
        },
        "baselib": {
            "release_tag": None,
            "latest_release_tag": None,
            "matches": None,
            "release_url": BASELIB_RELEASES_URL,
            "source_mode": _resolve_baselib_source_mode(),
            "decompiled_src_path": str(BASELIB_DECOMPILED_DIR),
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
    raise RuntimeError("BaseLib latest release does not contain a downloadable DLL asset")


def _directory_has_sources(path: Path) -> bool:
    return path.exists() and any(path.rglob("*.cs"))


def _resolve_game_source_mode() -> str:
    if _directory_has_sources(GAME_DECOMPILED_DIR):
        return "runtime_decompiled"
    if API_REF_PATH.exists():
        return "repo_reference"
    return "missing"


def _resolve_baselib_source_mode() -> str:
    if (BASELIB_DECOMPILED_DIR / "BaseLib.decompiled.cs").exists():
        return "runtime_decompiled"
    if BASELIB_FALLBACK_PATH.exists():
        return "repo_fallback"
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
        if not _directory_has_sources(GAME_DECOMPILED_DIR):
            payload["warnings"].append("游戏反编译源码目录为空，请先执行“更新知识库”")
        if not (BASELIB_DECOMPILED_DIR / "BaseLib.decompiled.cs").exists():
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
    payload["game"]["decompiled_src_path"] = manifest.get("game", {}).get("decompiled_src_path", str(GAME_DECOMPILED_DIR))
    payload["baselib"]["decompiled_src_path"] = manifest.get("baselib", {}).get("decompiled_src_path", str(BASELIB_DECOMPILED_DIR))

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

    if not _directory_has_sources(Path(payload["game"]["decompiled_src_path"])) or not Path(
        str(payload["baselib"]["decompiled_src_path"])
    ).joinpath("BaseLib.decompiled.cs").exists():
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


def get_active_game_decompiled_src_path() -> str | None:
    manifest = load_manifest()
    if manifest is not None:
        candidate = Path(str(manifest.get("game", {}).get("decompiled_src_path", "")).strip())
        if candidate.is_dir():
            return str(candidate)
    return get_decompiled_src_path()


def get_active_baselib_src_path() -> str:
    manifest = load_manifest()
    if manifest is not None:
        candidate_root = Path(str(manifest.get("baselib", {}).get("decompiled_src_path", "")).strip())
        candidate_file = candidate_root / "BaseLib.decompiled.cs"
        if candidate_file.exists():
            return str(candidate_file)
    return str(BASELIB_FALLBACK_PATH)


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
    with httpx.stream("GET", url, headers={"User-Agent": "AgentTheSpire-Codex"}, timeout=60.0) as response:
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
    config = get_config()
    sts2_path = str(config.get("sts2_path", "")).strip()
    if not sts2_path:
        raise RuntimeError("未配置 STS2 游戏路径，无法更新知识库")
    if not _has_ilspycmd():
        raise RuntimeError("未检测到 ilspycmd，请先放到项目目录或确保 ilspycmd 在 PATH 中")

    task.set_step("读取当前游戏版本")
    game_info = read_current_game_version(sts2_path)
    game_dll = _find_game_dll(sts2_path)

    task.set_step("反编译游戏源码")
    _run_ilspy_outputdir(game_dll, GAME_DECOMPILED_DIR)

    task.set_step("读取 Baselib latest release")
    release = fetch_latest_baselib_release()
    asset = select_baselib_asset(release)

    task.set_step("下载 Baselib.dll")
    baselib_dll_path = KNOWLEDGE_CACHE_DIR / str(asset["name"]).strip()
    _download_file(str(asset["browser_download_url"]).strip(), baselib_dll_path)

    task.set_step("反编译 Baselib")
    _reset_dir(BASELIB_DECOMPILED_DIR)
    _run_ilspy_to_single_file(baselib_dll_path, BASELIB_DECOMPILED_DIR / "BaseLib.decompiled.cs")

    manifest = {
        "schema_version": 1,
        "status": "fresh",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "game": {
            "sts2_path": sts2_path,
            "version": game_info["version"],
            "version_source": game_info["source"],
            "decompiled_src_path": str(GAME_DECOMPILED_DIR),
        },
        "baselib": {
            "release_tag": release.get("tag_name"),
            "release_published_at": release.get("published_at"),
            "asset_name": asset.get("name"),
            "downloaded_file_path": str(baselib_dll_path),
            "decompiled_src_path": str(BASELIB_DECOMPILED_DIR),
        },
        "last_check": {
            "checked_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "game_matches": True,
            "baselib_matches": True,
            "warnings": [],
        },
    }
    save_manifest(manifest)


def start_refresh_task() -> dict[str, Any]:
    task_id = uuid.uuid4().hex
    task = _RefreshTask(task_id)
    with _REFRESH_TASKS_LOCK:
        _REFRESH_TASKS[task_id] = task
    task.start()
    time.sleep(0.01)
    return task.snapshot()


def get_refresh_task(task_id: str) -> dict[str, Any]:
    with _REFRESH_TASKS_LOCK:
        task = _REFRESH_TASKS.get(task_id)
    if task is None:
        raise KeyError(task_id)
    return task.snapshot()
