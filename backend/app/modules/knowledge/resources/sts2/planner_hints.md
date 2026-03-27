## STS2 API reference — use exact names in implementation_notes

### Card pools — pick by character/theme
| Pool | For | Notes |
|---|---|---|
| `IroncladCardPool` | Ironclad | strength, block, exhaust |
| `SilentCardPool` | Silent | poison, shivs, discard |
| `DefectCardPool` | Defect | orbs, focus |
| `RegentCardPool` | Regent | stances, mantras |
| `NecrobinderCardPool` | Necrobinder | undead, death |
| `ColorlessCardPool` | All characters | truly neutral cards only |
| `MyCharCardPool : CustomCardPoolModel` | New mod character | create if adding a new character |

### Base classes (use exact names)
- Card: `CustomCardModel` — ctor params: `baseCost, CardType, CardRarity, TargetType`
- Relic: `CustomRelicModel` — abstract prop: `RelicRarity Rarity`
- Power: `PowerModel` — abstract props: `PowerType Type`, `PowerStackType StackType`
- Potion: `CustomPotionModel` — override `OnUse(ctx, Creature?)`
- Character: `PlaceholderCharacterModel` — set `PlaceholderID => "ironclad"`

### Key enums
- `CardType`: Attack, Skill, Power, Status, Curse
- `CardRarity`: Common, Uncommon, Rare, Ancient
- `TargetType`: None, Self, AnyEnemy, AllEnemies, RandomEnemy
- `PowerType`: Buff, Debuff
- `PowerStackType`: None, Counter, Single
- `RelicRarity`: Starter, Common, Uncommon, Rare, Shop, Event
- `CardKeyword`: Exhaust, Ethereal, Innate, Retain, Sly, Eternal, Unplayable

### Built-in powers (do NOT redefine, use directly)
Debuffs: `PoisonPower`, `VulnerablePower`, `WeakPower`, `FrailPower`, `SlowPower`
Buffs: `StrengthPower`, `DexterityPower`, `ThornsPower`, `RegenPower`, `ArtifactPower`, `IntangiblePower`, `BarricadePower`, `VigorPower`, `RagePower`, `EnragePower`, `LethalityPower`, `FocusPower`, `EchoFormPower`

### Key rules for implementation_notes
- Card numbers MUST use `CanonicalVars` with `DamageVar`/`BlockVar`/`PowerVar<T>` — no hardcoded literals
- Localization uses `{Damage}`, `{PoisonPower}` placeholders matching var names
- `ExtraHoverTips` with `HoverTipFactory.FromPower<T>()` for power keyword tooltips on cards
- Relic hooks need `ShouldReceiveCombatHooks => true`
- Common hooks: `AfterCardPlayed`, `AfterDamageGiven`, `AfterPlayerTurnStart`
- Cards need `[Pool(typeof(SomeCardPool))]` attribute
- Card effect: `protected override async Task OnPlay(PlayerChoiceContext ctx, CardPlay cardPlay)`
- NEVER `new XxxModel()` — use `ModelDb.Card<T>()` / `.ToMutable()`
