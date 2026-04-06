from __future__ import annotations

from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


def test_web_docker_template_keeps_onnxruntime_when_downgrading_rembg_gpu():
    dockerfile = (REPO_ROOT / "tools" / "latest" / "templates" / "web" / "Dockerfile").read_text(encoding="utf-8")

    assert 'line.replace("rembg[gpu]", "rembg", 1)' in dockerfile
    assert 'lines.append("onnxruntime>=1.18,<2.0")' in dockerfile


def test_workstation_docker_template_keeps_onnxruntime_when_downgrading_rembg_gpu():
    dockerfile = (REPO_ROOT / "tools" / "latest" / "templates" / "workstation" / "Dockerfile").read_text(encoding="utf-8")

    assert 'line.replace("rembg[gpu]", "rembg", 1)' in dockerfile
    assert 'lines.append("onnxruntime>=1.18,<2.0")' in dockerfile
