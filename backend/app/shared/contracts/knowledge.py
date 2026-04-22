from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal


@dataclass
class KnowledgeQuery:
    scenario: Literal["planner", "asset_codegen", "custom_code_codegen", "asset_group_codegen"]
    domain: str
    asset_type: str | None = None
    project_root: Path | None = None
    requirements: str | None = None
    item_name: str | None = None
    symbols: list[str] = field(default_factory=list)
    group_asset_types: list[str] = field(default_factory=list)


@dataclass
class KnowledgeFactItem:
    key: str
    title: str
    body: str
    priority: int
    evidence_paths: list[str] = field(default_factory=list)
    keywords: list[str] = field(default_factory=list)
    asset_types: list[str] = field(default_factory=list)


@dataclass
class KnowledgeGuidanceItem:
    key: str
    title: str
    body: str
    source_path: str
    asset_types: list[str] = field(default_factory=list)


@dataclass
class KnowledgeLookupItem:
    key: str
    title: str
    path: str
    note: str
    keywords: list[str] = field(default_factory=list)


@dataclass
class KnowledgePacket:
    domain: str
    scenario: str
    summary: str
    facts: list[KnowledgeFactItem] = field(default_factory=list)
    guidance: list[KnowledgeGuidanceItem] = field(default_factory=list)
    lookup: list[KnowledgeLookupItem] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
