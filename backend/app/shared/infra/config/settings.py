from __future__ import annotations

import json
import os
import subprocess
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional


_ENV_KEYS = {
    "llm.api_key": "SPIREFORGE_LLM_KEY",
    "image_gen.api_key": "SPIREFORGE_IMG_KEY",
    "image_gen.api_secret": "SPIREFORGE_IMG_SECRET",
}

CONFIG_PATH = Path(__file__).resolve().parents[5] / "config.json"

DEFAULT_LLM_CONFIG = {
    "mode": "agent_cli",
    "agent_backend": "claude",
    "provider": "anthropic",
    "model": "",
    "api_key": "",
    "base_url": "",
    "custom_prompt": "",
    "execution_mode": "legacy_direct",
}

DEFAULT_CONFIG = {
    "llm": DEFAULT_LLM_CONFIG,
    "migration": {
        "use_modular_single_workflow": False,
        "use_modular_batch_workflow": False,
        "use_unified_ws_contract": False,
        "platform_jobs_api_enabled": False,
        "platform_service_split_enabled": False,
        "platform_runner_enabled": False,
        "platform_events_v1_enabled": False,
        "platform_step_protocol_enabled": False,
    },
    "database": {
        "url": "",
        "echo": False,
        "pool_pre_ping": True,
    },
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


def normalize_llm_config(llm_cfg: Optional[dict[str, Any]]) -> dict[str, Any]:
    cfg = _deep_merge(DEFAULT_LLM_CONFIG, llm_cfg or {})
    mode = cfg.get("mode", "agent_cli")

    if mode == "claude_subscription":
        cfg["mode"] = "agent_cli"
        cfg["agent_backend"] = "claude"
    elif mode in {"api_key", "api", "litellm"}:
        cfg["mode"] = "api"
    elif mode != "agent_cli":
        cfg["mode"] = "agent_cli"

    if cfg.get("agent_backend") not in {"claude", "codex"}:
        cfg["agent_backend"] = "claude"

    if not cfg.get("provider"):
        cfg["provider"] = DEFAULT_LLM_CONFIG["provider"]

    return cfg


def normalize_config(config: Optional[dict[str, Any]]) -> dict[str, Any]:
    cfg = _deep_merge(DEFAULT_CONFIG, config or {})
    cfg["llm"] = normalize_llm_config(cfg.get("llm"))
    return cfg


def load_config() -> dict[str, Any]:
    if CONFIG_PATH.exists():
        with open(CONFIG_PATH, "r", encoding="utf-8") as file:
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
    CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(CONFIG_PATH, "w", encoding="utf-8") as file:
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
    def migration(self) -> dict[str, Any]:
        return self.raw["migration"]

    @property
    def database(self) -> dict[str, Any]:
        return self.raw["database"]

    @property
    def image_gen(self) -> dict[str, Any]:
        return self.raw["image_gen"]

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


def get_decompiled_src_path() -> Optional[str]:
    env_value = os.environ.get("SPIREFORGE_DECOMPILED_SRC", "")
    if env_value and Path(env_value).is_dir():
        return env_value

    cfg_value = get_config().get("decompiled_src_path", "")
    if cfg_value and Path(cfg_value).is_dir():
        return cfg_value

    return None
