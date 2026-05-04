from __future__ import annotations

from dataclasses import dataclass
from time import perf_counter

import httpx


@dataclass(slots=True)
class ServerCredentialHealthCheckResult:
    status: str
    error_code: str = ""
    error_message: str = ""
    latency_ms: int | None = None


class ServerCredentialHealthChecker:
    def __init__(self, timeout_seconds: float = 8.0) -> None:
        self.timeout_seconds = timeout_seconds

    def check(
        self,
        *,
        api_protocol: str,
        auth_type: str,
        credential: str,
        secret: str | None,
        api_base_url: str,
    ) -> ServerCredentialHealthCheckResult:
        if auth_type == "ak_sk":
            return ServerCredentialHealthCheckResult(
                status="degraded",
                error_code="unsupported_auth_type",
                error_message="manual health check does not support ak_sk credentials yet",
            )

        started = perf_counter()
        if api_protocol == "openai_compatible":
            url = f"{api_base_url.rstrip('/')}/models" if api_base_url else "https://api.openai.com/v1/models"
            headers = {"Authorization": f"Bearer {credential}"}
        elif api_protocol == "anthropic_compatible":
            base = api_base_url.rstrip("/") if api_base_url else "https://api.anthropic.com"
            url = f"{base}/v1/models"
            headers = {
                "x-api-key": credential,
                "anthropic-version": "2023-06-01",
            }
        else:
            return ServerCredentialHealthCheckResult(
                status="degraded",
                error_code="unsupported_api_protocol",
                error_message=f"manual health check does not support api_protocol: {api_protocol}",
            )

        try:
            response = httpx.get(url, headers=headers, timeout=self.timeout_seconds)
        except httpx.TimeoutException:
            return ServerCredentialHealthCheckResult(
                status="degraded",
                error_code="timeout",
                error_message="health check request timed out",
                latency_ms=int((perf_counter() - started) * 1000),
            )
        except httpx.HTTPError as exc:
            return ServerCredentialHealthCheckResult(
                status="degraded",
                error_code="network_error",
                error_message=str(exc),
                latency_ms=int((perf_counter() - started) * 1000),
            )

        latency_ms = int((perf_counter() - started) * 1000)
        content_type = response.headers.get("content-type", "")
        if "text/html" in content_type.lower():
            return ServerCredentialHealthCheckResult(
                status="degraded",
                error_code="api_protocol_mismatch",
                error_message="upstream returned HTML; api_base_url should point to the protocol API endpoint",
                latency_ms=latency_ms,
            )
        if response.status_code == 200:
            return ServerCredentialHealthCheckResult(status="healthy", latency_ms=latency_ms)
        if response.status_code in {401, 403}:
            return ServerCredentialHealthCheckResult(
                status="auth_failed",
                error_code=f"http_{response.status_code}",
                error_message="credential was rejected by upstream provider",
                latency_ms=latency_ms,
            )
        if response.status_code == 429:
            return ServerCredentialHealthCheckResult(
                status="rate_limited",
                error_code="http_429",
                error_message="upstream provider rate limited the request",
                latency_ms=latency_ms,
            )
        if response.status_code in {402}:
            return ServerCredentialHealthCheckResult(
                status="quota_exhausted",
                error_code="http_402",
                error_message="upstream provider reported quota exhaustion",
                latency_ms=latency_ms,
            )
        return ServerCredentialHealthCheckResult(
            status="degraded",
            error_code=f"http_{response.status_code}",
            error_message=f"upstream provider returned unexpected status {response.status_code}",
            latency_ms=latency_ms,
        )
