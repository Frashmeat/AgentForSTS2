"""Local pytest fixtures for backend tests."""

from __future__ import annotations

import shutil
import uuid
from pathlib import Path

import pytest

_TMP_ROOT = Path(__file__).resolve().parent / "_tmp"
collect_ignore = ["_tmp"]


@pytest.fixture
def tmp_path() -> Path:
    """Use a workspace-local temp dir to avoid broken system temp ACLs."""
    _TMP_ROOT.mkdir(parents=True, exist_ok=True)
    path = _TMP_ROOT / f"pytest-{uuid.uuid4().hex}"
    path.mkdir()
    try:
        yield path
    finally:
        shutil.rmtree(path, ignore_errors=True)
