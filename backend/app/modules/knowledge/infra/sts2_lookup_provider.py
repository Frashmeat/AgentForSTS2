from __future__ import annotations

from app.modules.knowledge.application.knowledge_facade import build_lookup_context
from app.modules.knowledge.infra import knowledge_runtime
from app.shared.contracts.knowledge import KnowledgeLookupItem, KnowledgeQuery


class Sts2LookupProvider:
    def build_lookup(self, _query: KnowledgeQuery) -> list[KnowledgeLookupItem]:
        knowledge_runtime.ensure_runtime_knowledge_seeded()
        lookup_context = build_lookup_context()
        items = [
            KnowledgeLookupItem(
                key="sts2.lookup.baselib",
                title="BaseLib local source",
                path=lookup_context["baselib_src_path"] or str(knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs"),
                note="Read this local decompiled source for `CustomCardModel`, `CustomPotionModel`, `PlaceholderCharacterModel`, and related BaseLib wrappers.",
                keywords=["BaseLib", "CustomCardModel", "CustomPotionModel", "PlaceholderCharacterModel"],
            )
        ]

        if lookup_context["game_source_mode"] == "runtime_decompiled":
            items.append(
                KnowledgeLookupItem(
                    key="sts2.lookup.game_runtime",
                    title="STS2 runtime knowledge directory",
                    path=lookup_context["game_path"],
                    note=(
                        "Read or grep this runtime knowledge directory directly. "
                        "Key subdirs include `MegaCrit.Sts2.Core.Commands`, `MegaCrit.Sts2.Core.Models.Cards`, "
                        "and `MegaCrit.Sts2.Core.CardSelection`."
                    ),
                    keywords=["runtime", "knowledge", "DamageCmd", "PowerCmd", "CardSelectorPrefs"],
                )
            )
        else:
            items.append(
                KnowledgeLookupItem(
                    key="sts2.lookup.game_fallback",
                    title="STS2 ilspy fallback",
                    path=lookup_context["ilspy_example_dll_path"],
                    note="If runtime-decompiled sources are missing, inspect the game DLL via `ilspycmd`.",
                    keywords=["ilspycmd", "sts2.dll"],
                )
            )

        items.append(
            KnowledgeLookupItem(
                key="sts2.lookup.guidance_resources",
                title="STS2 guidance resources",
                path=str(knowledge_runtime.active_resource_knowledge_dir()),
                note="Use these Markdown resources for conventions, common pitfalls, and summarized examples.",
                keywords=["guidance", "common.md", "card.md", "power.md", "relic.md", "custom_code.md"],
            )
        )
        return items
