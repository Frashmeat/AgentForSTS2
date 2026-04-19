from __future__ import annotations

import json
import os
import subprocess
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any


_ENV_KEYS = {
    "llm.api_key": "SPIREFORGE_LLM_KEY",
    "image_gen.api_key": "SPIREFORGE_IMG_KEY",
    "image_gen.api_secret": "SPIREFORGE_IMG_SECRET",
}

_CONFIG_PATH_ENV = "SPIREFORGE_CONFIG_PATH"
_SERVER_CREDENTIAL_SECRET_ENV = "SPIREFORGE_SERVER_CREDENTIAL_SECRET"
_APP_ROOT = Path(__file__).resolve().parents[5]


def _resolve_runtime_config_path() -> Path:
    # Dockerized release bundles mount the effective runtime config to /app/config.json.
    if _APP_ROOT.name == "backend":
        docker_mounted_config = _APP_ROOT.parent / "config.json"
        if docker_mounted_config.exists():
            return docker_mounted_config

    # Release bundles keep the real configs under <release>/runtime/<role>.config.json.
    if _APP_ROOT.name == "backend" and _APP_ROOT.parent.name in {"workstation", "web"} and _APP_ROOT.parent.parent.name == "services":
        role = _APP_ROOT.parent.name
        return _APP_ROOT.parent.parent.parent / "runtime" / f"{role}.config.json"

    return _APP_ROOT / "runtime" / "workstation.config.json"


RUNTIME_CONFIG_PATH = _resolve_runtime_config_path()
CONFIG_PATH = RUNTIME_CONFIG_PATH

DEFAULT_LLM_CONFIG = {
    "mode": "agent_cli",
    "agent_backend": "claude",
    "model": "",
    "api_key": "",
    "base_url": "",
    "custom_prompt": "",
    "execution_mode": "direct_execute",
}

DEFAULT_AUTH_CONFIG = {
    "session_cookie_name": "agentthespire_session",
    "session_secret": "",
    "session_cookie_secure": False,
    "session_cookie_samesite": "lax",
    "session_cookie_domain": "",
    "email_verification_code_ttl_seconds": 1800,
    "password_reset_code_ttl_seconds": 1800,
}

DEFAULT_RUNTIME_CONFIG = {
    "workstation": {
        "host": "127.0.0.1",
        "port": 7860,
        "cors_origins": [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:7860",
            "http://127.0.0.1:7860",
            "http://localhost:8080",
            "http://127.0.0.1:8080",
        ],
        "mount_frontend": True,
        "requires_database": False,
    },
    "web": {
        "host": "127.0.0.1",
        "port": 7870,
        "cors_origins": [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:7870",
            "http://127.0.0.1:7870",
            "http://localhost:8080",
            "http://127.0.0.1:8080",
        ],
        "mount_frontend": False,
        "requires_database": True,
    },
}

DEFAULT_CONFIG = {
    "llm": DEFAULT_LLM_CONFIG,
    "auth": DEFAULT_AUTH_CONFIG,
    "migration": {},
    "database": {
        "url": "",
        "echo": False,
        "pool_pre_ping": True,
    },
    "runtime": DEFAULT_RUNTIME_CONFIG,
    "approval": {
        "auto_execute_low_risk": False,
        "allowed_commands": [],
        "allowed_roots": [],
    },
    "image_gen": {
        "mode": "cloud",
        "provider": "bfl",
        "model": "flux.2-flex",
        "api_key": "",
        "api_secret": "",
        "batch_size": 3,
        "concurrency": 1,
        "rembg_model": "birefnet-general",
        "local": {
            "comfyui_url": "http://127.0.0.1:8188",
            "installed": False,
            "model_path": "",
        },
    },
    "godot_exe_path": "",
    "dotnet_path": "dotnet",
    "sts2_path": "",
    "default_project_root": "",
    "mod_template_path": "",
    "mod_projects": [],
    "active_project": "",
    "decompiled_src_path": "",
}


def _deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    result = deepcopy(base)
    for key, value in override.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = _deep_merge(result[key], value)
        else:
            result[key] = deepcopy(value)
    return result


def _normalize_execution_mode(value: Any) -> str:
    normalized = str(value or "").strip()
    if normalized == "approval_first":
        return "approval_first"
    if normalized in {"legacy_direct", "direct_execute", ""}:
        return "direct_execute"
    return "direct_execute"


def normalize_llm_config(llm_cfg: Optional[dict[str, Any]]) -> dict[str, Any]:
    cfg = _deep_merge(DEFAULT_LLM_CONFIG, llm_cfg or {})
    mode = cfg.get("mode", "agent_cli")

    if mode == "claude_subscription":
        cfg["mode"] = "agent_cli"
        cfg["agent_backend"] = "claude"
    elif mode in {"api_key", "api", "litellm", "claude_api"}:
        cfg["mode"] = "claude_api"
    elif mode != "agent_cli":
        cfg["mode"] = "agent_cli"

    if cfg.get("agent_backend") not in {"claude", "codex"}:
        cfg["agent_backend"] = "claude"
    cfg.pop("provider", None)
    cfg["execution_mode"] = _normalize_execution_mode(cfg.get("execution_mode"))

    return cfg


def normalize_config(config: Optional[dict[str, Any]]) -> dict[str, Any]:
    cfg = _deep_merge(DEFAULT_CONFIG, config or {})
    cfg["llm"] = normalize_llm_config(cfg.get("llm"))
    cfg["migration"] = {}
    cfg["runtime"] = {
        role: _deep_merge(DEFAULT_RUNTIME_CONFIG[role], cfg.get("runtime", {}).get(role, {}))
        for role in DEFAULT_RUNTIME_CONFIG
    }
    return cfg


def resolve_config_path(*, for_write: bool = False) -> Path:
    explicit_path = os.environ.get(_CONFIG_PATH_ENV, "").strip()
    if explicit_path:
        return Path(explicit_path).expanduser()

    return RUNTIME_CONFIG_PATH


def load_config() -> dict[str, Any]:
    config_path = resolve_config_path(for_write=False)
    if config_path.exists():
        # Windows PowerShell 写 JSON 时可能带 BOM。
        with open(config_path, "r", encoding="utf-8-sig") as file:
            saved = json.load(file)
        cfg = normalize_config(saved)
    else:
        cfg = normalize_config(None)

    for dotpath, envname in _ENV_KEYS.items():
        value = os.environ.get(envname, "")
        if value and not value.startswith("****"):
            section, key = dotpath.split(".")
            cfg[section][key] = value

    return cfg


def save_config(config: dict[str, Any]) -> None:
    normalized = normalize_config(config)
    config_path = resolve_config_path(for_write=True)
    config_path.parent.mkdir(parents=True, exist_ok=True)
    with open(config_path, "w", encoding="utf-8") as file:
        json.dump(normalized, file, indent=2, ensure_ascii=False)

    for dotpath, envname in _ENV_KEYS.items():
        section, key = dotpath.split(".")
        value = normalized.get(section, {}).get(key, "")
        if value and not value.startswith("****"):
            os.environ[envname] = value
            try:
                subprocess.run(["setx", envname, value], capture_output=True, check=False)
            except Exception:
                pass


@dataclass(slots=True)
class Settings:
    raw: dict[str, Any]

    @classmethod
    def from_dict(cls, config: Optional[dict[str, Any]]) -> "Settings":
        return cls(raw=normalize_config(config))

    @property
    def llm(self) -> dict[str, Any]:
        return self.raw["llm"]

    @property
    def approval(self) -> dict[str, Any]:
        return self.raw["approval"]

    @property
    def auth(self) -> dict[str, Any]:
        return self.raw["auth"]

    @property
    def migration(self) -> dict[str, Any]:
        return self.raw["migration"]

    @property
    def database(self) -> dict[str, Any]:
        return self.raw["database"]

    @property
    def image_gen(self) -> dict[str, Any]:
        return self.raw["image_gen"]

    def get_runtime(self, role: str) -> dict[str, Any]:
        runtime_cfg = self.raw.get("runtime", {})
        return deepcopy(runtime_cfg.get(role, {}))

    def get_server_credential_secret(self) -> str:
        configured = os.environ.get(_SERVER_CREDENTIAL_SECRET_ENV, "").strip()
        if configured:
            return configured
        return str(self.auth.get("session_secret", "")).strip()

    def validate_for_role(self, role: str) -> list[str]:
        runtime = self.get_runtime(role)
        errors: list[str] = []

        cors_origins = runtime.get("cors_origins", [])
        if not isinstance(cors_origins, list) or not cors_origins:
            errors.append(f"runtime.{role}.cors_origins must contain at least one origin")

        if runtime.get("requires_database", False):
            database_url = str(self.database.get("url", "")).strip()
            if not database_url:
                errors.append(f"database.url is required for {role} runtime")

        session_cookie_name = str(self.auth.get("session_cookie_name", "")).strip()
        if not session_cookie_name:
            errors.append("auth.session_cookie_name is required")

        if role == "web":
            session_secret = str(self.auth.get("session_secret", "")).strip()
            if not session_secret:
                errors.append("auth.session_secret is required for web runtime")

        return errors

    def to_dict(self) -> dict[str, Any]:
        return deepcopy(self.raw)


_config: Optional[dict[str, Any]] = None


def get_config() -> dict[str, Any]:
    global _config
    if _config is None:
        _config = load_config()
    return _config


def update_config(patch: dict[str, Any]) -> dict[str, Any]:
    global _config
    cfg = get_config()
    _config = normalize_config(_deep_merge(cfg, patch))
    save_config(_config)
    return _config
