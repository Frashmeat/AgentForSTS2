from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Literal

AssetItemType = Literal["card", "card_fullscreen", "relic", "power", "character", "custom_code"]


@dataclass
class PlanItem:
    id: str
    type: AssetItemType
    name: str
    name_zhs: str = ""
    description: str = ""
    implementation_notes: str = ""
    needs_image: bool = True
    image_description: str = ""
    depends_on: list[str] = field(default_factory=list)
    provided_image_b64: str = ""

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class ModPlan:
    mod_name: str
    summary: str
    items: list[PlanItem]

    def to_dict(self) -> dict:
        return {
            "mod_name": self.mod_name,
            "summary": self.summary,
            "items": [item.to_dict() for item in self.items],
        }
