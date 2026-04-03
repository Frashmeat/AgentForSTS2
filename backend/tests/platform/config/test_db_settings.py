import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.shared.infra.config.settings import Settings, normalize_config


def test_settings_exposes_database_defaults():
    settings = Settings.from_dict(None)

    assert settings.database == {
        "url": "",
        "echo": False,
        "pool_pre_ping": True,
    }


def test_normalize_config_merges_database_overrides():
    config = normalize_config(
        {
            "database": {
                "url": "postgresql+psycopg://user:pass@localhost:5432/ats",
                "echo": True,
            }
        }
    )

    assert config["database"]["url"] == "postgresql+psycopg://user:pass@localhost:5432/ats"
    assert config["database"]["echo"] is True
    assert config["database"]["pool_pre_ping"] is True
