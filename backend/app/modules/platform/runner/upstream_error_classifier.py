from __future__ import annotations

import re
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class UpstreamErrorClassification:
    reason_code: str
    upstream_category: str
    reason_message: str
    retryable: bool
    http_status: int | None
    provider_error_code: str
    raw_error: str


_STATUS_PATTERN = re.compile(r"\b(?:status(?: code)?|http(?: status)?)[=: ]+(\d{3})\b", re.IGNORECASE)


def _extract_http_status(text: str) -> int | None:
    match = _STATUS_PATTERN.search(text)
    if match is None:
        for status in (400, 401, 403, 404, 429):
            if re.search(rf"\b{status}\b", text):
                return status
        return None
    return int(match.group(1))


def _contains_any(text: str, markers: tuple[str, ...]) -> bool:
    return any(marker in text for marker in markers)


def classify_upstream_error(error: Exception) -> UpstreamErrorClassification:
    raw_error = str(error)
    text = raw_error.lower()
    http_status = _extract_http_status(text)

    if _contains_any(text, ("content_filter", "content filter", "content policy", "safety policy")):
        return UpstreamErrorClassification(
            reason_code="upstream_content_policy_blocked",
            upstream_category="content_policy",
            reason_message="上游模型判定本次请求触发内容策略，请调整描述后重试。",
            retryable=False,
            http_status=http_status,
            provider_error_code="content_filter",
            raw_error=raw_error,
        )

    if http_status == 429 or _contains_any(text, ("rate limit", "rate_limit", "too many requests", "quota exceeded")):
        return UpstreamErrorClassification(
            reason_code="upstream_rate_limited",
            upstream_category="rate_limited",
            reason_message="上游服务限流或额度不足，请稍后重试或切换执行配置。",
            retryable=True,
            http_status=http_status or 429,
            provider_error_code="rate_limited",
            raw_error=raw_error,
        )

    if http_status in {401, 403} or _contains_any(
        text,
        (
            "unauthorized",
            "forbidden",
            "permission",
            "not authorized",
            "access denied",
            "model access",
            "region unavailable",
            "unsupported country",
        ),
    ):
        return UpstreamErrorClassification(
            reason_code="upstream_auth_or_region_blocked",
            upstream_category="auth_or_region",
            reason_message="当前服务器凭据、模型权限、base_url 或区域配置不可用，请管理员检查执行配置。",
            retryable=False,
            http_status=http_status,
            provider_error_code="auth_or_region",
            raw_error=raw_error,
        )

    if http_status == 400 or _contains_any(text, ("bad request", "invalid request", "unsupported parameter")):
        return UpstreamErrorClassification(
            reason_code="upstream_bad_request",
            upstream_category="bad_request",
            reason_message="上游拒绝了请求参数，请管理员检查模型、base_url 或 provider 兼容配置。",
            retryable=False,
            http_status=http_status or 400,
            provider_error_code="bad_request",
            raw_error=raw_error,
        )

    if "request was blocked" in text or "blocked" in text:
        return UpstreamErrorClassification(
            reason_code="upstream_gateway_blocked",
            upstream_category="gateway_blocked",
            reason_message="上游网关拒绝了请求，可能与供应商网关、代理、账号策略或模型权限有关，请管理员查看诊断日志。",
            retryable=False,
            http_status=http_status,
            provider_error_code="request_blocked",
            raw_error=raw_error,
        )

    return UpstreamErrorClassification(
        reason_code="upstream_unclassified_error",
        upstream_category="unclassified",
        reason_message="上游模型调用失败，请管理员查看诊断日志确认具体原因。",
        retryable=True,
        http_status=http_status,
        provider_error_code="",
        raw_error=raw_error,
    )
