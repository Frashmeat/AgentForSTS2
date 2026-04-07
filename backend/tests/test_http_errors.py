from __future__ import annotations

import sys
from pathlib import Path

from fastapi import FastAPI, HTTPException
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.shared.infra.http_errors import install_http_error_handlers


def test_http_exception_uses_structured_error_envelope():
    app = FastAPI()
    install_http_error_handlers(app)

    @app.get("/known")
    def known_error():
        raise HTTPException(status_code=400, detail="authentication required")

    with TestClient(app) as client:
        response = client.get("/known")

    assert response.status_code == 400
    assert response.json() == {
        "error": {
            "code": "http_400",
            "message": "authentication required",
            "detail": "authentication required",
        }
    }


def test_unhandled_exception_uses_safe_fallback_message():
    app = FastAPI()
    install_http_error_handlers(app)

    @app.get("/boom")
    def boom():
        raise RuntimeError("database password leaked")

    with TestClient(app, raise_server_exceptions=False) as client:
        response = client.get("/boom")

    assert response.status_code == 500
    assert response.json() == {
        "error": {
            "code": "internal_server_error",
            "message": "服务端发生异常，请稍后重试",
            "detail": None,
        }
    }
