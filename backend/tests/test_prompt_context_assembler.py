import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.contracts.knowledge import (
    KnowledgeFactItem,
    KnowledgeGuidanceItem,
    KnowledgeLookupItem,
    KnowledgePacket,
)
from app.shared.prompting import PromptContextAssembler


def test_prompt_context_assembler_sorts_facts_by_priority_then_key():
    assembler = PromptContextAssembler()
    packet = KnowledgePacket(
        domain="sts2",
        scenario="asset_codegen",
        summary="summary",
        facts=[
            KnowledgeFactItem(key="b", title="B", body="body-b", priority=20),
            KnowledgeFactItem(key="a", title="A", body="body-a", priority=10),
        ],
    )

    context = assembler.assemble(packet)

    assert context["facts"].index("### A") < context["facts"].index("### B")


def test_prompt_context_assembler_includes_warning_block_when_present():
    assembler = PromptContextAssembler()
    packet = KnowledgePacket(
        domain="sts2",
        scenario="asset_codegen",
        summary="summary",
        warnings=["MainFile.cs not found"],
    )

    context = assembler.assemble(packet)

    assert "### Warnings" in context["knowledge_warnings"]
    assert "MainFile.cs not found" in context["knowledge_warnings"]


def test_prompt_context_assembler_renders_guidance_and_lookup_sections():
    assembler = PromptContextAssembler()
    packet = KnowledgePacket(
        domain="sts2",
        scenario="asset_codegen",
        summary="summary",
        guidance=[
            KnowledgeGuidanceItem(
                key="guidance.card",
                title="Card guidance",
                body="Use card guidance.",
                source_path="runtime/knowledge/resources/sts2/card.md",
            )
        ],
        lookup=[
            KnowledgeLookupItem(
                key="lookup.game",
                title="Game runtime",
                path="runtime/knowledge/game",
                note="Read or grep directly.",
            )
        ],
    )

    context = assembler.assemble(packet)

    assert "Card guidance" in context["guidance"]
    assert "runtime/knowledge/resources/sts2/card.md" in context["guidance"]
    assert "Game runtime" in context["lookup"]
    assert "runtime/knowledge/game" in context["lookup"]


def test_prompt_context_assembler_handles_empty_guidance_and_lookup():
    assembler = PromptContextAssembler()
    packet = KnowledgePacket(domain="sts2", scenario="asset_codegen", summary="summary")

    context = assembler.assemble(packet)

    assert context["guidance"] == ""
    assert context["lookup"] == ""
