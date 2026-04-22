from __future__ import annotations

from app.shared.contracts.knowledge import (
    KnowledgeFactItem,
    KnowledgeGuidanceItem,
    KnowledgeLookupItem,
    KnowledgePacket,
)


class PromptContextAssembler:
    def assemble(self, packet: KnowledgePacket) -> dict[str, str]:
        return {
            "facts": self._render_facts(packet.facts),
            "guidance": self._render_guidance(packet.guidance),
            "lookup": self._render_lookup(packet.lookup),
            "knowledge_warnings": self._render_warnings(packet.warnings),
            "summary": packet.summary.strip(),
        }

    def _render_facts(self, facts: list[KnowledgeFactItem]) -> str:
        if not facts:
            return ""
        ordered = sorted(facts, key=lambda item: (item.priority, item.key))
        return "\n\n".join(self._render_fact_item(item) for item in ordered)

    def _render_guidance(self, guidance: list[KnowledgeGuidanceItem]) -> str:
        if not guidance:
            return ""
        ordered = sorted(guidance, key=lambda item: item.key)
        return "\n\n".join(self._render_guidance_item(item) for item in ordered)

    def _render_lookup(self, lookup: list[KnowledgeLookupItem]) -> str:
        if not lookup:
            return ""
        ordered = sorted(lookup, key=lambda item: item.key)
        return "\n\n".join(self._render_lookup_item(item) for item in ordered)

    @staticmethod
    def _render_warnings(warnings: list[str]) -> str:
        if not warnings:
            return ""
        lines = ["### Warnings"]
        lines.extend(f"- {warning}" for warning in warnings)
        return "\n".join(lines)

    @staticmethod
    def _render_fact_item(item: KnowledgeFactItem) -> str:
        lines = [f"### {item.title}", item.body.strip()]
        if item.evidence_paths:
            lines.append("Evidence paths:")
            lines.extend(f"- `{path}`" for path in item.evidence_paths)
        return "\n".join(lines).strip()

    @staticmethod
    def _render_guidance_item(item: KnowledgeGuidanceItem) -> str:
        lines = [f"### {item.title}", item.body.strip()]
        if item.source_path:
            lines.append(f"Source: `{item.source_path}`")
        return "\n".join(lines).strip()

    @staticmethod
    def _render_lookup_item(item: KnowledgeLookupItem) -> str:
        lines = [f"### {item.title}", f"Path: `{item.path}`"]
        if item.note:
            lines.append(item.note.strip())
        return "\n".join(lines).strip()
