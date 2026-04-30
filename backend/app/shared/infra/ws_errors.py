from __future__ import annotations

import json
from typing import Any

DEFAULT_WS_ERROR_MESSAGE = "请求失败，请稍后重试"


def _pick_text(*values: object) -> str | None:
    for value in values:
        if not isinstance(value, str):
            continue
        trimmed = value.strip()
        if trimmed:
            return trimmed
    return None


def build_ws_error_payload(
    *,
    code: str,
    message: str | None = None,
    detail: str | None = None,
    request_id: str | None = None,
    traceback: str | None = None,
    fallback_message: str = DEFAULT_WS_ERROR_MESSAGE,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    resolved_message = _pick_text(message, detail, fallback_message) or fallback_message
    payload: dict[str, Any] = {
        "code": code,
        "message": resolved_message,
        "detail": _pick_text(detail, resolved_message),
    }
    if request_id:
        payload["request_id"] = request_id
    if traceback:
        payload["traceback"] = traceback
    if extra:
        payload.update(extra)
    return payload


async def send_ws_error(
    ws,
    *,
    code: str,
    message: str | None = None,
    detail: str | None = None,
    request_id: str | None = None,
    traceback: str | None = None,
    fallback_message: str = DEFAULT_WS_ERROR_MESSAGE,
    event: str = "error",
    extra: dict[str, Any] | None = None,
) -> None:
    payload = build_ws_error_payload(
        code=code,
        message=message,
        detail=detail,
        request_id=request_id,
        traceback=traceback,
        fallback_message=fallback_message,
        extra=extra,
    )
    await ws.send_text(json.dumps({"event": event, **payload}))
