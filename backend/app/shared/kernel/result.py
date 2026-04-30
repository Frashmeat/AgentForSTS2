from __future__ import annotations

from typing import Generic, TypeVar

T = TypeVar("T")


class Result(Generic[T]):
    def __init__(self, ok: bool, value: T | None = None, error: Exception | None = None) -> None:
        self.ok = ok
        self.value = value
        self.error = error

    @classmethod
    def ok(cls, value: T | None = None) -> Result[T]:
        return cls(ok=True, value=value)

    @classmethod
    def fail(cls, error: Exception) -> Result[T]:
        return cls(ok=False, error=error)
