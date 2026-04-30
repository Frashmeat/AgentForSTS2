import sys
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).parent.parent))

import image.postprocess as postprocess


def test_get_gpu_providers_returns_cpu_when_windows_cuda_dll_load_fails(monkeypatch, tmp_path: Path):
    provider_dll = tmp_path / "onnxruntime" / "capi" / "onnxruntime_providers_cuda.dll"
    provider_dll.parent.mkdir(parents=True, exist_ok=True)
    provider_dll.write_text("stub", encoding="utf-8")

    calls = {"providers": 0}

    fake_ort = SimpleNamespace(
        __file__=str(tmp_path / "onnxruntime" / "__init__.py"),
        get_available_providers=lambda: calls.__setitem__("providers", calls["providers"] + 1)
        or ["CUDAExecutionProvider", "CPUExecutionProvider"],
    )

    monkeypatch.setitem(sys.modules, "onnxruntime", fake_ort)
    monkeypatch.setattr(postprocess.os, "name", "nt", raising=False)
    monkeypatch.setattr(
        postprocess.ctypes, "WinDLL", lambda _path: (_ for _ in ()).throw(OSError("missing cublasLt64_12.dll"))
    )

    providers = postprocess._get_gpu_providers()

    assert providers == ["CPUExecutionProvider"]
    assert calls["providers"] == 0


def test_get_gpu_providers_returns_cuda_when_windows_cuda_dll_loads(monkeypatch, tmp_path: Path):
    provider_dll = tmp_path / "onnxruntime" / "capi" / "onnxruntime_providers_cuda.dll"
    provider_dll.parent.mkdir(parents=True, exist_ok=True)
    provider_dll.write_text("stub", encoding="utf-8")

    fake_ort = SimpleNamespace(
        __file__=str(tmp_path / "onnxruntime" / "__init__.py"),
        get_available_providers=lambda: ["CUDAExecutionProvider", "CPUExecutionProvider"],
    )

    monkeypatch.setitem(sys.modules, "onnxruntime", fake_ort)
    monkeypatch.setattr(postprocess.os, "name", "nt", raising=False)
    monkeypatch.setattr(postprocess.ctypes, "WinDLL", lambda _path: object())

    providers = postprocess._get_gpu_providers()

    assert providers == ["CUDAExecutionProvider", "CPUExecutionProvider"]


def test_get_gpu_providers_returns_cpu_when_cuda_provider_not_available(monkeypatch):
    fake_ort = SimpleNamespace(
        __file__="unused",
        get_available_providers=lambda: ["CPUExecutionProvider"],
    )

    monkeypatch.setitem(sys.modules, "onnxruntime", fake_ort)
    monkeypatch.setattr(postprocess.os, "name", "posix", raising=False)

    providers = postprocess._get_gpu_providers()

    assert providers == ["CPUExecutionProvider"]
