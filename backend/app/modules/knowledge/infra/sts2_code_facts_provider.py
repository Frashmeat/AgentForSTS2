from __future__ import annotations

import re
from pathlib import Path

from app.modules.knowledge.application.knowledge_facade import build_lookup_context
from app.modules.knowledge.infra import knowledge_runtime
from app.shared.contracts.knowledge import KnowledgeFactItem, KnowledgeQuery


class Sts2CodeFactsProvider:
    def build_facts(self, query: KnowledgeQuery) -> tuple[list[KnowledgeFactItem], list[str]]:
        knowledge_runtime.ensure_runtime_knowledge_seeded()
        warnings: list[str] = []
        facts: list[KnowledgeFactItem] = [self._runtime_knowledge_fact()]
        facts.extend(self._project_facts(query.project_root, warnings))

        for asset_type in self._iter_asset_types(query):
            builder = self._type_fact_builders().get(asset_type)
            if builder is None:
                warnings.append(f"No type-specific code facts found for asset type: {asset_type}")
                continue
            facts.extend(builder())

        facts.extend(self._requirement_facts(query.requirements or ""))
        return self._dedupe(facts), warnings

    def _runtime_knowledge_fact(self) -> KnowledgeFactItem:
        lookup = build_lookup_context()
        body = (
            f"Use runtime knowledge as the source of truth. "
            f"Game path: `{lookup['game_path'] or knowledge_runtime.active_game_knowledge_dir()}`. "
            f"BaseLib path: `{lookup['baselib_src_path'] or (knowledge_runtime.active_baselib_knowledge_dir() / 'BaseLib.decompiled.cs')}`."
        )
        return KnowledgeFactItem(
            key="sts2.runtime.knowledge_paths",
            title="Runtime knowledge source",
            body=body,
            priority=10,
            evidence_paths=[
                str(knowledge_runtime.active_game_knowledge_dir()),
                str(knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs"),
            ],
            keywords=["runtime", "knowledge", "baselib", "sts2"],
            asset_types=["card", "power", "relic", "custom_code", "character"],
        )

    def _project_facts(self, project_root: Path | None, warnings: list[str]) -> list[KnowledgeFactItem]:
        if project_root is None:
            return []

        facts: list[KnowledgeFactItem] = []
        main_file = project_root / "MainFile.cs"
        csproj = next(project_root.glob("*.csproj"), None) if project_root.exists() else None
        if not main_file.exists():
            warnings.append(f"MainFile.cs not found under project root: {project_root}")
        else:
            text = main_file.read_text(encoding="utf-8", errors="replace")
            namespace = self._extract_group(text, r"\bnamespace\s+([A-Za-z0-9_.]+)")
            mod_id = self._extract_group(text, r'ModId\s*(?:=>|=)\s*"([^"]+)"')
            patch_all = "PatchAll(" in text
            body = (
                f"Project namespace: `{namespace or 'unknown'}`. "
                f"ModId: `{mod_id or 'unknown'}`. "
                f"Harmony PatchAll present: `{patch_all}`. "
                f"Godot resource root should stay under `{project_root.name}/`."
            )
            facts.append(
                KnowledgeFactItem(
                    key="sts2.project.mainfile_context",
                    title="Project MainFile context",
                    body=body,
                    priority=20,
                    evidence_paths=[str(main_file)],
                    keywords=["MainFile", "namespace", "ModId", "PatchAll"],
                    asset_types=["card", "power", "relic", "custom_code", "character"],
                )
            )

        if csproj is None:
            warnings.append(f"No .csproj file found under project root: {project_root}")
        else:
            facts.append(
                KnowledgeFactItem(
                    key="sts2.project.csproj_context",
                    title="Project build context",
                    body=(
                        f"Project file: `{csproj.name}`. "
                        f"Localization folders should live under `{project_root.name}/localization/eng` "
                        f"and `{project_root.name}/localization/zhs`."
                    ),
                    priority=25,
                    evidence_paths=[str(csproj)],
                    keywords=["csproj", "localization", "resource root"],
                    asset_types=["card", "power", "relic", "custom_code", "character"],
                )
            )

        return facts

    @staticmethod
    def _extract_group(text: str, pattern: str) -> str:
        match = re.search(pattern, text)
        return match.group(1).strip() if match else ""

    def _iter_asset_types(self, query: KnowledgeQuery) -> list[str]:
        asset_types: list[str] = []
        if query.asset_type:
            asset_types.append(query.asset_type)
        asset_types.extend(query.group_asset_types)
        if not asset_types and query.scenario == "custom_code_codegen":
            asset_types.append("custom_code")
        ordered: list[str] = []
        for asset_type in asset_types:
            if asset_type not in ordered:
                ordered.append(asset_type)
        return ordered

    def _type_fact_builders(self) -> dict[str, callable]:
        return {
            "card": self._card_facts,
            "card_fullscreen": self._card_facts,
            "power": self._power_facts,
            "relic": self._relic_facts,
            "custom_code": self._custom_code_facts,
            "character": self._character_facts,
        }

    def _card_facts(self) -> list[KnowledgeFactItem]:
        resource_root = knowledge_runtime.active_resource_knowledge_dir()
        card_doc = str(resource_root / "card.md")
        baselib = str(knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs")
        api_ref = str(knowledge_runtime.active_game_knowledge_dir() / knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE.name)
        return [
            KnowledgeFactItem(
                key="sts2.card.base_class",
                title="Card base class and registration",
                body=(
                    "Use `CustomCardModel` as the default card base class. "
                    "Cards must declare `[Pool(typeof(...))]` so BaseLib can auto-add them into the correct pool."
                ),
                priority=30,
                evidence_paths=[card_doc, baselib],
                keywords=["CustomCardModel", "Pool", "CardType", "CardRarity", "TargetType"],
                asset_types=["card", "card_fullscreen"],
            ),
            KnowledgeFactItem(
                key="sts2.card.command_usage",
                title="Card damage and power application",
                body=(
                    "Use `DamageCmd.Attack(...)` to deal damage from cards and `PowerCmd.Apply<T>(...)` to apply powers. "
                    "Do not call direct HP gain/loss helpers from card logic."
                ),
                priority=40,
                evidence_paths=[card_doc, api_ref],
                keywords=["DamageCmd", "PowerCmd", "CreatureCmd"],
                asset_types=["card", "card_fullscreen"],
            ),
        ]

    def _power_facts(self) -> list[KnowledgeFactItem]:
        resource_root = knowledge_runtime.active_resource_knowledge_dir()
        power_doc = str(resource_root / "power.md")
        api_ref = str(knowledge_runtime.active_game_knowledge_dir() / knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE.name)
        return [
            KnowledgeFactItem(
                key="sts2.power.base_class",
                title="Power base class",
                body=(
                    "Use `PowerModel` directly for powers. There is no `CustomPowerModel` in the current STS2 stack."
                ),
                priority=30,
                evidence_paths=[power_doc],
                keywords=["PowerModel", "PowerType"],
                asset_types=["power"],
            ),
            KnowledgeFactItem(
                key="sts2.power.command_usage",
                title="Power apply and lifecycle commands",
                body=(
                    "Use `PowerCmd.Apply<T>(...)` to apply powers, `PowerCmd.TickDownDuration(this)` for duration countdown, "
                    "and `PowerCmd.Remove(this)` for removal. Use `CreatureCmd` for damage-style side effects."
                ),
                priority=40,
                evidence_paths=[power_doc, api_ref],
                keywords=["PowerCmd", "CreatureCmd", "StrengthPower", "DexterityPower"],
                asset_types=["power"],
            ),
        ]

    def _relic_facts(self) -> list[KnowledgeFactItem]:
        relic_doc = str(knowledge_runtime.active_resource_knowledge_dir() / "relic.md")
        return [
            KnowledgeFactItem(
                key="sts2.relic.base_class",
                title="Relic base class and pool registration",
                body=(
                    "Use `CustomRelicModel` as the default relic base class. "
                    "Relics enter pools via `[Pool(typeof(SharedRelicPool))]` and BaseLib auto registration."
                ),
                priority=30,
                evidence_paths=[relic_doc],
                keywords=["CustomRelicModel", "SharedRelicPool", "Pool"],
                asset_types=["relic"],
            ),
        ]

    def _custom_code_facts(self) -> list[KnowledgeFactItem]:
        resource_root = knowledge_runtime.active_resource_knowledge_dir()
        custom_doc = str(resource_root / "custom_code.md")
        baselib = str(knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs")
        api_ref = str(knowledge_runtime.active_game_knowledge_dir() / knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE.name)
        return [
            KnowledgeFactItem(
                key="sts2.custom_code.patching",
                title="Harmony patching baseline",
                body=(
                    "Custom code defaults to Harmony-based extension points. "
                    "Reuse existing `PatchAll()` wiring in `MainFile.cs`; do not add duplicate manual patch registration."
                ),
                priority=30,
                evidence_paths=[custom_doc, baselib],
                keywords=["HarmonyPatch", "PatchAll", "Prefix", "Postfix"],
                asset_types=["custom_code"],
            ),
            KnowledgeFactItem(
                key="sts2.custom_code.commands",
                title="Custom code command entrypoints",
                body=(
                    "Common runtime actions should still flow through `DamageCmd`, `PowerCmd`, `CreatureCmd`, "
                    "and `CardSelectorPrefs` rather than custom one-off helpers."
                ),
                priority=40,
                evidence_paths=[custom_doc, api_ref],
                keywords=["DamageCmd", "PowerCmd", "CreatureCmd", "CardSelectorPrefs"],
                asset_types=["custom_code"],
            ),
        ]

    def _character_facts(self) -> list[KnowledgeFactItem]:
        resource_root = knowledge_runtime.active_resource_knowledge_dir()
        character_doc = str(resource_root / "character.md")
        baselib = str(knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs")
        api_ref = str(knowledge_runtime.active_game_knowledge_dir() / knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE.name)
        return [
            KnowledgeFactItem(
                key="sts2.character.base_class",
                title="Character base class",
                body=(
                    "Character implementations can start from `PlaceholderCharacterModel` when art is not ready, "
                    "and expand toward full `CharacterModel` hooks later."
                ),
                priority=30,
                evidence_paths=[character_doc, baselib, api_ref],
                keywords=["PlaceholderCharacterModel", "CharacterModel"],
                asset_types=["character"],
            )
        ]

    def _requirement_facts(self, requirements: str) -> list[KnowledgeFactItem]:
        text = requirements.lower()
        facts: list[KnowledgeFactItem] = []
        resource_root = knowledge_runtime.active_resource_knowledge_dir()
        api_ref = str(knowledge_runtime.active_game_knowledge_dir() / knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE.name)
        custom_doc = str(resource_root / "custom_code.md")
        triggers = [
            (
                ["选牌", "选择牌", "upgrade", "remove", "exhaust"],
                KnowledgeFactItem(
                    key="sts2.selection.card_selector_prefs",
                    title="Card selection structure",
                    body=(
                        "When the design requires choosing cards, use `CardSelectorPrefs` as the default selection control structure."
                    ),
                    priority=50,
                    evidence_paths=[custom_doc, api_ref],
                    keywords=["CardSelectorPrefs"],
                    asset_types=["card", "custom_code"],
                ),
            ),
            (
                ["伤害", "attack", "aoe", "随机敌人"],
                KnowledgeFactItem(
                    key="sts2.damage.damage_cmd",
                    title="Damage command",
                    body="Route card or scripted damage through `DamageCmd.Attack(...)`.",
                    priority=55,
                    evidence_paths=[str(resource_root / "card.md"), api_ref],
                    keywords=["DamageCmd"],
                    asset_types=["card", "custom_code"],
                ),
            ),
            (
                ["施加力量", "力量", "敏捷", "buff", "debuff", "中毒"],
                KnowledgeFactItem(
                    key="sts2.power.power_cmd",
                    title="Power application command",
                    body="Apply buffs and debuffs through `PowerCmd.Apply<T>(...)` instead of ad hoc state mutation.",
                    priority=56,
                    evidence_paths=[str(resource_root / "power.md"), api_ref],
                    keywords=["PowerCmd", "StrengthPower", "DexterityPower"],
                    asset_types=["card", "power", "custom_code"],
                ),
            ),
            (
                ["事件", "开场", "neow", "patch", "hook"],
                KnowledgeFactItem(
                    key="sts2.patch.harmony_hooks",
                    title="Harmony hook reminder",
                    body=(
                        "If the feature targets an existing event or lifecycle hook, prefer a focused `HarmonyPatch` on the concrete method."
                    ),
                    priority=57,
                    evidence_paths=[custom_doc, str(knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs")],
                    keywords=["HarmonyPatch", "PatchAll"],
                    asset_types=["custom_code"],
                ),
            ),
        ]

        for trigger_words, fact in triggers:
            if any(word in text for word in trigger_words):
                facts.append(fact)
        return facts

    @staticmethod
    def _dedupe(items: list[KnowledgeFactItem]) -> list[KnowledgeFactItem]:
        result: list[KnowledgeFactItem] = []
        seen: set[str] = set()
        for item in items:
            if item.key in seen:
                continue
            seen.add(item.key)
            result.append(item)
        return result
