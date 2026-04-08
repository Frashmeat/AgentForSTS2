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

    legacy_config = tmp_path / "config.json"
    legacy_config.write_text('{"llm":{"agent_backend":"claude"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "LEGACY_CONFIG_PATH", legacy_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.delenv("SPIREFORGE_CONFIG_PATH", raising=False)
    _reset_cached_config(monkeypatch)

    config = settings_module.load_config()

    assert config["llm"]["agent_backend"] == "codex"


def test_load_config_falls_back_to_legacy_root_config(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"
    legacy_config = tmp_path / "config.json"
    legacy_config.write_text('{"llm":{"agent_backend":"codex"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "LEGACY_CONFIG_PATH", legacy_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.delenv("SPIREFORGE_CONFIG_PATH", raising=False)
    _reset_cached_config(monkeypatch)

    config = settings_module.load_config()

    assert config["llm"]["agent_backend"] == "codex"


def test_load_config_uses_explicit_env_override(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"
    runtime_config.parent.mkdir(parents=True, exist_ok=True)
    runtime_config.write_text('{"llm":{"agent_backend":"claude"}}\n', encoding="utf-8")

    override_config = tmp_path / "custom.json"
    override_config.write_text('{"llm":{"agent_backend":"codex"}}\n', encoding="utf-8")

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "LEGACY_CONFIG_PATH", tmp_path / "config.json")
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.setenv("SPIREFORGE_CONFIG_PATH", str(override_config))
    _reset_cached_config(monkeypatch)

    config = settings_module.load_config()

    assert config["llm"]["agent_backend"] == "codex"


def test_save_config_writes_runtime_workstation_config_by_default(monkeypatch, tmp_path: Path):
    runtime_config = tmp_path / "runtime" / "workstation.config.json"
    legacy_config = tmp_path / "config.json"

    monkeypatch.setattr(settings_module, "RUNTIME_CONFIG_PATH", runtime_config)
    monkeypatch.setattr(settings_module, "LEGACY_CONFIG_PATH", legacy_config)
    monkeypatch.setattr(settings_module, "CONFIG_PATH", runtime_config)
    monkeypatch.delenv("SPIREFORGE_CONFIG_PATH", raising=False)
    monkeypatch.setattr(settings_module.subprocess, "run", lambda *args, **kwargs: None)
    _reset_cached_config(monkeypatch)

    settings_module.save_config({"llm": {"agent_backend": "codex"}})

    assert runtime_config.exists()
    assert not legacy_config.exists()
    assert '"agent_backend": "codex"' in runtime_config.read_text(encoding="utf-8")
