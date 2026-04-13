import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.infra.config import settings as settings_module


def _reset_cached_config(monkeypatch):
    monkeypatch.setattr(settings_module, "_config", None)


def test_load_config_prefers_runtime_workstation_config(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"
    runtime_config.parent.mkdir(parents=True, exist_ok=True)
    runtime_config.write_text('{"llm":{"agent_backend":"codex"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.delenv("SPIREFORGE_CONFIG_PATH", raising=False)
    _reset_cached_config(monkeypatch)

    config = settings_module.load_config()

    assert config["llm"]["agent_backend"] == "codex"


def test_load_config_ignores_root_config_when_runtime_config_is_missing(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"
    legacy_config = tmp_path / "config.json"
    legacy_config.write_text('{"llm":{"agent_backend":"codex"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.delenv("SPIREFORGE_CONFIG_PATH", raising=False)
    _reset_cached_config(monkeypatch)

    config = settings_module.load_config()

    assert settings_module.resolve_config_path(for_write=False) == runtime_config
    assert config["llm"]["agent_backend"] == settings_module.DEFAULT_LLM_CONFIG["agent_backend"]


def test_load_config_uses_explicit_env_override(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"
    runtime_config.parent.mkdir(parents=True, exist_ok=True)
    runtime_config.write_text('{"llm":{"agent_backend":"claude"}}\n', encoding="utf-8")

    override_config = tmp_path / "custom.json"
    override_config.write_text('{"llm":{"agent_backend":"codex"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.setenv("SPIREFORGE_CONFIG_PATH", str(override_config))
    _reset_cached_config(monkeypatch)

    config = settings_module.load_config()

    assert config["llm"]["agent_backend"] == "codex"


def test_save_config_writes_runtime_workstation_config_by_default(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.delenv("SPIREFORGE_CONFIG_PATH", raising=False)
    monkeypatch.setattr(settings_module.subprocess, "run", lambda *args, **kwargs: None)
    _reset_cached_config(monkeypatch)

    settings_module.save_config({"llm": {"agent_backend": "codex"}})

    assert runtime_config.exists()
    assert '"agent_backend": "codex"' in runtime_config.read_text(encoding="utf-8")


def test_resolve_runtime_config_path_prefers_docker_mounted_config(monkeypatch, tmp_path: Path):
    app_root = tmp_path / "app" / "backend"
    app_root.mkdir(parents=True, exist_ok=True)
    docker_config = tmp_path / "app" / "config.json"
    docker_config.write_text('{"database":{"url":"sqlite+pysqlite:///:memory:"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "_APP_ROOT", app_root)

    assert settings_module._resolve_runtime_config_path() == docker_config


def test_resolve_runtime_config_path_uses_role_specific_release_runtime_config(monkeypatch, tmp_path: Path):
    backend_root = tmp_path / "release" / "services" / "web" / "backend"
    backend_root.mkdir(parents=True, exist_ok=True)
    runtime_config = tmp_path / "release" / "runtime" / "web.config.json"

    monkeypatch.setattr(settings_module, "_APP_ROOT", backend_root)

    assert settings_module._resolve_runtime_config_path() == runtime_config


def test_resolve_runtime_config_path_defaults_to_repo_runtime_config(monkeypatch, tmp_path: Path):
    repo_root = tmp_path / "repo"
    repo_root.mkdir(parents=True, exist_ok=True)

    monkeypatch.setattr(settings_module, "_APP_ROOT", repo_root)

    assert settings_module._resolve_runtime_config_path() == repo_root / "runtime" / "workstation.config.json"
