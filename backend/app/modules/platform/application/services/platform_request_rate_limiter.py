from __future__ import annotations

from collections import defaultdict, deque
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from threading import Lock


@dataclass(slots=True)
class _RateLimitRule:
    limit: int
    window_seconds: int
    message: str


class PlatformRequestRateLimitExceededError(ValueError):
    pass


class PlatformRequestRateLimiter:
    _RULES: dict[str, _RateLimitRule] = {
        "create_job": _RateLimitRule(
            limit=5,
            window_seconds=60,
            message="too many platform job create requests for user: limit 5 per 60 seconds",
        ),
        "start_job": _RateLimitRule(
            limit=5,
            window_seconds=60,
            message="too many platform job start requests for user: limit 5 per 60 seconds",
        ),
    }

    def __init__(self, *, now_factory: Callable[[], datetime] | None = None) -> None:
        self._now_factory = now_factory or (lambda: datetime.now(UTC))
        self._events: dict[tuple[int, str], deque[datetime]] = defaultdict(deque)
        self._lock = Lock()

    def check_and_record(self, *, user_id: int, action: str) -> None:
        rule = self._RULES.get(action)
        if rule is None:
            raise ValueError(f"platform request rate limiter action is not supported: {action}")

        now = self._now_factory()
        cutoff = now - timedelta(seconds=rule.window_seconds)
        key = (user_id, action)
        with self._lock:
            queue = self._events[key]
            while queue and queue[0] <= cutoff:
                queue.popleft()
            if len(queue) >= rule.limit:
                raise PlatformRequestRateLimitExceededError(rule.message)
            queue.append(now)
