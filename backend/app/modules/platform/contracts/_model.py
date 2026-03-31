from __future__ import annotations

from dataclasses import MISSING, fields, is_dataclass
from typing import Any, get_args, get_origin, get_type_hints


def _convert_value(value: Any, annotation: Any) -> Any:
    origin = get_origin(annotation)

    if origin is list:
        (item_type,) = get_args(annotation)
        return [_convert_value(item, item_type) for item in value]

    if isinstance(annotation, type) and issubclass(annotation, ModelBase):
        if isinstance(value, annotation):
            return value
        return annotation.model_validate(value)

    return value


class ModelBase:
    @classmethod
    def model_validate(cls, payload: dict[str, Any]) -> "ModelBase":
        hints = get_type_hints(cls)
        kwargs: dict[str, Any] = {}

        for item in fields(cls):
            name = item.name
            if name in payload:
                kwargs[name] = _convert_value(payload[name], hints.get(name, item.type))
            elif item.default is not MISSING:
                kwargs[name] = item.default
            elif item.default_factory is not MISSING:  # type: ignore[attr-defined]
                kwargs[name] = item.default_factory()  # type: ignore[misc]
            else:
                raise KeyError(f"missing required field: {name}")

        return cls(**kwargs)

    def model_dump(self, *, exclude: set[str] | None = None, exclude_none: bool = False) -> dict[str, Any]:
        result: dict[str, Any] = {}
        exclude = exclude or set()

        for item in fields(self):
            if item.name in exclude:
                continue
            value = getattr(self, item.name)
            if exclude_none and value is None:
                continue
            result[item.name] = self._dump_value(value, exclude_none=exclude_none)
        return result

    @classmethod
    def _dump_value(cls, value: Any, *, exclude_none: bool = False) -> Any:
        if isinstance(value, ModelBase):
            return value.model_dump(exclude_none=exclude_none)
        if isinstance(value, list):
            return [cls._dump_value(item, exclude_none=exclude_none) for item in value]
        if is_dataclass(value):
            return {
                item.name: cls._dump_value(getattr(value, item.name), exclude_none=exclude_none)
                for item in fields(value)
                if not (exclude_none and getattr(value, item.name) is None)
            }
        return value
