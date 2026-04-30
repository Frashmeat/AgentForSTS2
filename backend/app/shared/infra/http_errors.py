from __future__ import annotations

import logging
from typing import Any

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse
from starlette.exceptions import HTTPException as StarletteHTTPException

logger = logging.getLogger(__name__)

_INTERNAL_SERVER_ERROR_CODE = "internal_server_error"
_INTERNAL_SERVER_ERROR_MESSAGE = "服务端发生异常，请稍后重试"


def _pick_text(*values: object) -> str | None:
    for value in values:
        if isinstance(value, str):
            text = value.strip()
            if text:
                return text
    return None


def _extract_message(detail: Any) -> tuple[str | None, str | None]:
    if isinstance(detail, dict):
        nested_error = detail.get("error")
        if isinstance(nested_error, dict):
            nested_message = _pick_text(
                nested_error.get("message"),
                nested_error.get("detail"),
            )
            if nested_message:
                return nested_message, _pick_text(nested_error.get("detail"), nested_message)

        top_level_message = _pick_text(
            detail.get("message"),
            detail.get("detail"),
            detail.get("error"),
        )
        if top_level_message:
            return top_level_message, _pick_text(detail.get("detail"), top_level_message)

    if isinstance(detail, list):
        issues = []
        for item in detail:
            if not isinstance(item, dict):
                continue
            location = item.get("loc")
            location_text = ".".join(str(part) for part in location) if isinstance(location, list) else None
            message = _pick_text(item.get("msg"))
            if location_text and message:
                issues.append(f"{location_text}: {message}")
            elif message:
                issues.append(message)
        if issues:
            summary = "; ".join(issues)
            return summary, summary

    text = _pick_text(detail)
    if text:
        return text, text

    return None, None


def build_error_envelope(
    *,
    code: str,
    message: str,
    detail: str | None,
    request_id: str | None = None,
    meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "code": code,
        "message": message,
        "detail": detail,
    }
    if request_id:
        payload["request_id"] = request_id
    if meta:
        payload["meta"] = meta
    return {"error": payload}


def _read_request_id(request: Request) -> str | None:
    state_request_id = getattr(request.state, "request_id", None)
    if isinstance(state_request_id, str) and state_request_id.strip():
        return state_request_id.strip()

    header_request_id = request.headers.get("x-request-id")
    if isinstance(header_request_id, str) and header_request_id.strip():
        return header_request_id.strip()

    return None


async def _http_exception_handler(request: Request, exc: HTTPException) -> JSONResponse:
    message, detail = _extract_message(exc.detail)
    resolved_message = message or f"请求失败（HTTP {exc.status_code}）"
    return JSONResponse(
        status_code=exc.status_code,
        content=build_error_envelope(
            code=f"http_{exc.status_code}",
            message=resolved_message,
            detail=detail,
            request_id=_read_request_id(request),
        ),
    )


async def _unhandled_exception_handler(request: Request, exc: Exception) -> JSONResponse:
    logger.exception("Unhandled application error on %s %s", request.method, request.url.path, exc_info=exc)
    return JSONResponse(
        status_code=500,
        content=build_error_envelope(
            code=_INTERNAL_SERVER_ERROR_CODE,
            message=_INTERNAL_SERVER_ERROR_MESSAGE,
            detail=None,
            request_id=_read_request_id(request),
        ),
    )


def install_http_error_handlers(app: FastAPI) -> None:
    app.add_exception_handler(HTTPException, _http_exception_handler)
    app.add_exception_handler(StarletteHTTPException, _http_exception_handler)
    app.add_exception_handler(Exception, _unhandled_exception_handler)
