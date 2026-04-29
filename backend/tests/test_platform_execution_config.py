import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from config import Settings, normalize_config


def test_normalize_config_adds_platform_execution_defaults():
    cfg = normalize_config(None)

    assert cfg["platform_execution"] == {
        "workstation_url": "http://127.0.0.1:7860",
        "workstation_config_path": "runtime/workstation.config.json",
        "auto_start": True,
        "control_token_env": "ATS_WORKSTATION_CONTROL_TOKEN",
        "dispatch_timeout_seconds": 10,
        "poll_interval_seconds": 2,
        "execution_timeout_seconds": 180,
        "max_concurrent_text": 2,
        "max_concurrent_code": 2,
        "max_concurrent_workspace_writes_per_ref": 1,
        "max_concurrent_deploy_per_target": 1,
    }


def test_settings_exposes_platform_execution_copy():
    settings = Settings.from_dict(
        {
            "platform_execution": {
                "workstation_url": "http://127.0.0.1:7861",
                "max_concurrent_text": 3,
            }
        }
    )

    platform_execution = settings.platform_execution
    platform_execution["workstation_url"] = "mutated"

    assert settings.platform_execution["workstation_url"] == "http://127.0.0.1:7861"
    assert settings.platform_execution["max_concurrent_text"] == 3
    assert settings.platform_execution["max_concurrent_code"] == 2
