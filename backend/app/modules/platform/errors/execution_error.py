from __future__ import annotations

import re
import traceback
from dataclasses import dataclass, field
from typing import Any

PLATFORM_ERROR_SCHEMA_VERSION = "platform_error.v1"
_TEXT_LIMIT = 1200
_SECRET_PATTERNS = (
    re.compile(r"(Bearer\s+)[A-Za-z0-9._~+/=-]+", re.IGNORECASE),
    re.compile(r"(sk-[A-Za-z0-9_-]{8,})", re.IGNORECASE),
    re.compile(r"((?:api[_-]?key|authorization|credential|secret|token)[=:]\s*)[^\s,;]+", re.IGNORECASE),
)


@dataclass(frozen=True, slots=True)
class PlatformErrorEnvelope:
    origin: str
    runtime_surface: str
    component: str
    operation: str
    category: str
    reason_code: str
    message: str
    developer_message: str = ""
    retryable: bool = False
    step_id: str = ""
    step_type: str = ""
    job_id: int | None = None
    job_item_id: int | None = None
    execution_id: int | None = None
    provider: str = ""
    model: str = ""
    http_status: int | None = None
    provider_error_code: str = ""
    log_hint: dict[str, str] = field(default_factory=dict)
    diagnostic: dict[str, object] = field(default_factory=dict)

    def to_payload(self) -> dict[str, object]:
        payload: dict[str, object] = {
            "schema_version": PLATFORM_ERROR_SCHEMA_VERSION,
            "origin": self.origin,
            "runtime_surface": self.runtime_surface,
            "component": self.component,
            "operation": self.operation,
            "category": self.category,
            "reason_code": self.reason_code,
            "message": self.message,
            "developer_message": self.developer_message,
            "retryable": self.retryable,
            "step_id": self.step_id,
            "step_type": self.step_type,
            "provider": self.provider,
            "model": self.model,
            "provider_error_code": self.provider_error_code,
            "log_hint": dict(self.log_hint),
            "diagnostic": dict(self.diagnostic),
        }
        if self.job_id is not None:
            payload["job_id"] = self.job_id
        if self.job_item_id is not None:
            payload["job_item_id"] = self.job_item_id
        if self.execution_id is not None:
            payload["execution_id"] = self.execution_id
        if self.http_status is not None:
            payload["http_status"] = self.http_status
        return payload

    def with_context(
        self,
        *,
        step_id: str = "",
        step_type: str = "",
        job_id: int | None = None,
        job_item_id: int | None = None,
        execution_id: int | None = None,
        runtime_surface: str = "",
    ) -> PlatformErrorEnvelope:
        return PlatformErrorEnvelope(
            origin=self.origin,
            runtime_surface=runtime_surface or self.runtime_surface,
            component=self.component,
            operation=self.operation,
            category=self.category,
            reason_code=self.reason_code,
            message=self.message,
            developer_message=self.developer_message,
            retryable=self.retryable,
            step_id=step_id or self.step_id,
            step_type=step_type or self.step_type,
            job_id=job_id if job_id is not None else self.job_id,
            job_item_id=job_item_id if job_item_id is not None else self.job_item_id,
            execution_id=execution_id if execution_id is not None else self.execution_id,
            provider=self.provider,
            model=self.model,
            http_status=self.http_status,
            provider_error_code=self.provider_error_code,
            log_hint=dict(self.log_hint),
            diagnostic=dict(self.diagnostic),
        )


class PlatformExecutionError(RuntimeError):
    def __init__(self, envelope: PlatformErrorEnvelope) -> None:
        super().__init__(envelope.message)
        self.envelope = envelope

    def to_error_payload(self) -> dict[str, object]:
        return self.envelope.to_payload()


def sanitize_diagnostic_text(value: object, limit: int = _TEXT_LIMIT) -> str:
    text = str(value or "").replace("\r", "\\r").replace("\n", "\\n").strip()
    for pattern in _SECRET_PATTERNS:
        text = pattern.sub(lambda match: f"{match.group(1) if match.groups() else ''}[redacted]", text)
    return text[-limit:] if len(text) > limit else text


def build_platform_error(
    *,
    origin: str,
    runtime_surface: str,
    component: str,
    operation: str,
    category: str,
    reason_code: str,
    message: str,
    developer_message: object = "",
    retryable: bool = False,
    step_id: str = "",
    step_type: str = "",
    job_id: int | None = None,
    job_item_id: int | None = None,
    execution_id: int | None = None,
    provider: str = "",
    model: str = "",
    http_status: int | None = None,
    provider_error_code: str = "",
    log_hint: dict[str, str] | None = None,
    diagnostic: dict[str, object] | None = None,
) -> PlatformErrorEnvelope:
    safe_diagnostic = {
        str(key): sanitize_diagnostic_text(value) if isinstance(value, (str, bytes)) else value
        for key, value in dict(diagnostic or {}).items()
    }
    return PlatformErrorEnvelope(
        origin=origin,
        runtime_surface=runtime_surface,
        component=component,
        operation=operation,
        category=category,
        reason_code=reason_code,
        message=message,
        developer_message=sanitize_diagnostic_text(developer_message),
        retryable=retryable,
        step_id=step_id,
        step_type=step_type,
        job_id=job_id,
        job_item_id=job_item_id,
        execution_id=execution_id,
        provider=provider,
        model=model,
        http_status=http_status,
        provider_error_code=provider_error_code,
        log_hint=dict(log_hint or {}),
        diagnostic=safe_diagnostic,
    )


def platform_error_from_payload(payload: dict[str, object]) -> PlatformErrorEnvelope | None:
    if str(payload.get("schema_version", "")).strip() != PLATFORM_ERROR_SCHEMA_VERSION:
        return None
    return PlatformErrorEnvelope(
        origin=str(payload.get("origin") or "web"),
        runtime_surface=str(payload.get("runtime_surface") or "web"),
        component=str(payload.get("component") or ""),
        operation=str(payload.get("operation") or ""),
        category=str(payload.get("category") or "internal_error"),
        reason_code=str(payload.get("reason_code") or "web_internal_error"),
        message=str(payload.get("message") or "任务执行失败"),
        developer_message=str(payload.get("developer_message") or ""),
        retryable=bool(payload.get("retryable")),
        step_id=str(payload.get("step_id") or ""),
        step_type=str(payload.get("step_type") or ""),
        job_id=_optional_int(payload.get("job_id")),
        job_item_id=_optional_int(payload.get("job_item_id")),
        execution_id=_optional_int(payload.get("execution_id")),
        provider=str(payload.get("provider") or ""),
        model=str(payload.get("model") or ""),
        http_status=_optional_int(payload.get("http_status")),
        provider_error_code=str(payload.get("provider_error_code") or ""),
        log_hint=_string_dict(payload.get("log_hint")),
        diagnostic=dict(payload.get("diagnostic")) if isinstance(payload.get("diagnostic"), dict) else {},
    )


def error_payload_from_exception(
    error: Exception,
    *,
    runtime_surface: str,
    component: str,
    operation: str,
    step_id: str = "",
    step_type: str = "",
    job_id: int | None = None,
    job_item_id: int | None = None,
    execution_id: int | None = None,
) -> dict[str, object]:
    payload_builder = getattr(error, "to_error_payload", None)
    if callable(payload_builder):
        candidate = payload_builder()
        if isinstance(candidate, dict):
            existing = platform_error_from_payload(candidate)
            if existing is not None:
                return existing.with_context(
                    step_id=step_id,
                    step_type=step_type,
                    job_id=job_id,
                    job_item_id=job_item_id,
                    execution_id=execution_id,
                    runtime_surface=runtime_surface,
                ).to_payload()
            if candidate:
                return _legacy_payload_to_platform_error(
                    candidate,
                    error,
                    runtime_surface=runtime_surface,
                    component=component,
                    operation=operation,
                    step_id=step_id,
                    step_type=step_type,
                    job_id=job_id,
                    job_item_id=job_item_id,
                    execution_id=execution_id,
                ).to_payload()

    return build_platform_error(
        origin=runtime_surface if runtime_surface in {"web", "web_workstation", "local_workstation"} else "web",
        runtime_surface=runtime_surface,
        component=component,
        operation=operation,
        category="internal_error",
        reason_code=f"{runtime_surface}_internal_error" if runtime_surface else "web_internal_error",
        message="任务执行失败，请管理员查看诊断日志。",
        developer_message=str(error),
        retryable=True,
        step_id=step_id,
        step_type=step_type,
        job_id=job_id,
        job_item_id=job_item_id,
        execution_id=execution_id,
        diagnostic={"exception_type": type(error).__name__, "traceback": traceback.format_exc()},
    ).to_payload()


def _legacy_payload_to_platform_error(
    payload: dict[str, object],
    error: Exception,
    *,
    runtime_surface: str,
    component: str,
    operation: str,
    step_id: str,
    step_type: str,
    job_id: int | None,
    job_item_id: int | None,
    execution_id: int | None,
) -> PlatformErrorEnvelope:
    reason_code = str(payload.get("reason_code") or "upstream_unclassified_error").strip()
    upstream_category = str(payload.get("upstream_category") or "").strip()
    is_upstream = reason_code.startswith("upstream_") or reason_code.startswith("llm_") or bool(upstream_category)
    return build_platform_error(
        origin="upstream" if is_upstream else runtime_surface,
        runtime_surface=runtime_surface,
        component=component,
        operation=operation,
        category=_legacy_category(payload, is_upstream),
        reason_code=reason_code,
        message=str(payload.get("reason_message") or str(error) or "任务执行失败"),
        developer_message=str(payload.get("raw_error") or error),
        retryable=bool(payload.get("retryable")),
        step_id=step_id,
        step_type=step_type,
        job_id=job_id,
        job_item_id=job_item_id,
        execution_id=execution_id,
        http_status=_optional_int(payload.get("http_status")),
        provider_error_code=str(payload.get("provider_error_code") or ""),
        log_hint={"primary": f"{runtime_surface}_log"},
        diagnostic=payload,
    )


def _legacy_category(payload: dict[str, object], is_upstream: bool) -> str:
    upstream_category = str(payload.get("upstream_category") or "").strip()
    if upstream_category:
        return upstream_category
    if is_upstream:
        return "upstream"
    return "internal_error"


def _optional_int(value: Any) -> int | None:
    if isinstance(value, bool) or value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _string_dict(value: object) -> dict[str, str]:
    if not isinstance(value, dict):
        return {}
    return {str(key): str(item) for key, item in value.items()}
