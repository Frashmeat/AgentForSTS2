from dataclasses import dataclass, field


@dataclass(slots=True)
class CodegenRequest:
    prompt: str
    context: dict[str, object] = field(default_factory=dict)
