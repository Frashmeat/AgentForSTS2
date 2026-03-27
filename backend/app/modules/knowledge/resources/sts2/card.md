## Cards

### Base class: CustomCardModel
```csharp
using BaseLib.Abstracts;
using BaseLib.Utils;                               // PoolAttribute
using MegaCrit.Sts2.Core.Commands;                // DamageCmd
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.GameActions.Multiplayer; // PlayerChoiceContext
using MegaCrit.Sts2.Core.Localization.DynamicVars;// DamageVar, BlockVar
using MegaCrit.Sts2.Core.Models.CardPools;        // IroncladCardPool, SilentCardPool, etc.
using MegaCrit.Sts2.Core.ValueProps;              // ValueProp

namespace MyMod.Cards;

[Pool(typeof(IroncladCardPool))]              // REQUIRED — crashes without it
public class DarkBlade() : CustomCardModel(
    baseCost: 1,                              // ← named parameter is "baseCost" (NOT "cost")
    type: CardType.Attack,
    rarity: CardRarity.Common,
    target: TargetType.AnyEnemy)
{
    // Declare damage/block values here — engine reads them for card description too
    protected override IEnumerable<DynamicVar> CanonicalVars =>
        [new DamageVar(8m, ValueProp.Move)];   // 8 base damage, scales with Strength

    public override string PortraitPath       => "DarkBlade.png".CardImagePath();
    public override string CustomPortraitPath => "DarkBlade.png".BigCardImagePath();
    public override string BetaPortraitPath   => "DarkBlade.png".CardImagePath();

    // Dealing damage — use DamageCmd.Attack, never call GainHp/LoseHp directly
    protected override async Task OnPlay(PlayerChoiceContext choiceContext, CardPlay cardPlay)
    {
        ArgumentNullException.ThrowIfNull(cardPlay.Target, "cardPlay.Target");
        await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue)
            .FromCard(this)
            .Targeting(cardPlay.Target)
            .Execute(choiceContext);
    }

    // Turn-end-in-hand effect (MUST also override HasTurnEndInHandEffect => true)
    public override bool HasTurnEndInHandEffect => true;
    public override async Task OnTurnEndInHand(PlayerChoiceContext choiceContext)
    {
        await base.OnTurnEndInHand(choiceContext);
    }

    // Upgrade logic — use IsUpgraded (NOT UpgradeLevel, NOT Level)
    protected override void OnUpgrade()
    {
        base.DynamicVars.Damage.UpgradeValueBy(3m);   // 8 → 11
    }
    // Read upgrade state anywhere: if (IsUpgraded) { ... }
}
```

### DamageCmd — the only correct way to deal damage from a card
```csharp
// Single target (AnyEnemy / RandomEnemy)
await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue)
    .FromCard(this)
    .Targeting(cardPlay.Target)
    .Execute(choiceContext);

// All enemies (TargetType.AllEnemies) — iterate CombatState.Enemies
// cardPlay.Target is null for AllEnemies; read enemies from context instead

// Random opponent (no specific target needed — use with TargetType.None)
await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue)
    .FromCard(this)
    .TargetingRandomOpponents(base.CombatState!)   // picks one random living enemy
    .Execute(choiceContext);

// Multi-hit (plays N times):
await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue)
    .FromCard(this)
    .Targeting(cardPlay.Target)
    .WithHitCount(IsUpgraded ? 2 : 1)              // hit N times
    .Execute(choiceContext);

// X-cost cards: read captured X from energy
int xValue = base.DynamicVars.XValue.BaseValue;   // or EnergyCost.CapturedXValue

// With hit visual effect (optional):
await DamageCmd.Attack(dmg).FromCard(this).Targeting(target)
    .WithHitFx("vfx/vfx_attack_slash")
    .Execute(choiceContext);
```

### Cross-card shared state (e.g. "combo counter")
```csharp
// Static counter class — plain static, no Harmony needed
public static class ComboCounter
{
    public static int Count { get; private set; }
    public static void Increment() => Count++;
    public static void Reset() => Count = 0;
}

// Reset at turn end — override AfterTurnEnd on any card in the project.
// AfterTurnEnd fires on ALL cards in ANY combat pile (Draw/Hand/Discard/Exhaust).
// side == CombatSide.Player means it's the player's turn ending.
public override Task AfterTurnEnd(PlayerChoiceContext choiceContext, CombatSide side)
{
    if (side == CombatSide.Player) ComboCounter.Reset();
    return base.AfterTurnEnd(choiceContext, side);
}
// Also reset on AfterCombatEnd to avoid state leaking between fights.
public override Task AfterCombatEnd(CombatRoom room)
{
    ComboCounter.Reset();
    return base.AfterCombatEnd(room);
}
```

### DynamicVar types and live card numbers
All values shown on the card face must be declared as `DynamicVar` in `CanonicalVars`. The engine
automatically recalculates and shows updated values (e.g. 4 instead of 6 when Weakened).
- `DamageVar(baseValue, ValueProp.Move)` — scales with Strength/Weak/Vulnerable; accessed as `base.DynamicVars.Damage`
- `BlockVar(baseValue, ValueProp.Move)` — scales with Dexterity/Frail; accessed as `base.DynamicVars.Block`
- `PowerVar<T>(baseValue)` — amount for a power (e.g. poison stacks); accessed by name e.g. `base.DynamicVars["PoisonPower"]`
- `XVar(...)` — for X-cost cards

**Always** use `base.DynamicVars.Damage.BaseValue` (not a hardcoded literal) when calling DamageCmd.

```csharp
// Card that deals damage AND applies poison — full CanonicalVars + ExtraHoverTips pattern:
using MegaCrit.Sts2.Core.Localization.DynamicVars; // DamageVar, PowerVar
using MegaCrit.Sts2.Core.Models.Powers;            // PoisonPower
using MegaCrit.Sts2.Core.HoverTips;               // HoverTipFactory

protected override IEnumerable<DynamicVar> CanonicalVars =>
    [new DamageVar(6m, ValueProp.Move), new PowerVar<PoisonPower>(3m)];

// Keyword tooltip: shows "Poison" as a linked keyword on the card
protected override IEnumerable<IHoverTip> ExtraHoverTips =>
    [HoverTipFactory.FromPower<PoisonPower>()];

// In OnPlay:
await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue).FromCard(this).Targeting(cardPlay.Target).Execute(choiceContext);
await PowerCmd.Apply<PoisonPower>(cardPlay.Target, base.DynamicVars["PoisonPower"].BaseValue, base.Owner.Creature, this);

// Upgrade:
protected override void OnUpgrade()
{
    base.DynamicVars.Damage.UpgradeValueBy(2m);
    base.DynamicVars["PoisonPower"].UpgradeValueBy(1m);
}
```

### Localization with dynamic values
Use `{VarName}` placeholders in description text — the engine substitutes the live computed value.
Variable names: `Damage` (DamageVar), `Block` (BlockVar), `PoisonPower` (PowerVar\<PoisonPower\>), etc.

```json
{
  "MY_MOD-BLOOD_LANCE.description": "造成 {Damage} 点伤害。施加 {PoisonPower} 层[毒]。",
  "MY_MOD-BLOOD_LANCE.upgrade_description": "造成 {Damage} 点伤害。施加 {PoisonPower} 层[毒]。"
}
```
- `{Damage}` renders the live value (already modified by Weak/Strength etc.)
- `[毒]` or `[Poison]` renders the keyword with tooltip (BBCode-style tag the game supports for keyword highlighting — this is different from `[]` used in plain text which must be avoided)
- Do NOT hardcode numbers in localization if a `DynamicVar` exists for that value — use `{VarName}` instead

### ExtraHoverTips — keyword tooltips on the card
```csharp
// Single power keyword:
protected override IEnumerable<IHoverTip> ExtraHoverTips =>
    [HoverTipFactory.FromPower<PoisonPower>()];

// Multiple power keywords:
protected override IEnumerable<IHoverTip> ExtraHoverTips =>
    [HoverTipFactory.FromPower<VulnerablePower>(), HoverTipFactory.FromPower<WeakPower>()];
```

### Card mechanic keywords (CanonicalKeywords)
These appear as highlighted keyword banners on the card. Use `CanonicalKeywords` property:
```csharp
using MegaCrit.Sts2.Core.Entities.Cards;  // CardKeyword

// Single keyword:
public override IEnumerable<CardKeyword> CanonicalKeywords =>
    [CardKeyword.Exhaust];

// Multiple keywords:
public override IEnumerable<CardKeyword> CanonicalKeywords =>
    [CardKeyword.Exhaust, CardKeyword.Innate];

// Add a keyword on upgrade:
protected override void OnUpgrade()
{
    AddKeyword(CardKeyword.Retain);
    base.DynamicVars.Damage.UpgradeValueBy(3m);
}
```

| `CardKeyword` value | Meaning |
|---|---|
| `Exhaust` | Removed from play (sent to exhaust pile) after played |
| `Ethereal` | Removed from hand at end of turn if not played |
| `Innate` | Starts in opening hand each combat |
| `Retain` | NOT discarded at end of turn |
| `Sly` | Appears in hand after the first turn (delayed draw) |
| `Eternal` | Stays in hand even after played |
| `Unplayable` | Cannot be played (effect card / curse) |

Displayed order: Ethereal/Sly/Retain/Innate/Unplayable shown **before** description; Exhaust/Eternal shown **after**.

### Power keywords on cards (ExtraHoverTips)
To show a power name as a linked keyword tooltip on the card face, override `ExtraHoverTips`:
```csharp
using MegaCrit.Sts2.Core.HoverTips;  // HoverTipFactory, IHoverTip

// Single power keyword:
protected override IEnumerable<IHoverTip> ExtraHoverTips =>
    [HoverTipFactory.FromPower<PoisonPower>()];

// Multiple power keywords:
protected override IEnumerable<IHoverTip> ExtraHoverTips =>
    [HoverTipFactory.FromPower<VulnerablePower>(), HoverTipFactory.FromPower<WeakPower>()];
```
Without this, the power name in the card description is plain text with no tooltip.

### Card pools — choose based on which character should get this card
All pools are in `MegaCrit.Sts2.Core.Models.CardPools`. Pick the pool matching the card's theme/character:

| Pool class | Character | Theme |
|---|---|---|
| `IroncladCardPool` | Ironclad | Strength, block, self-damage, exhaust |
| `SilentCardPool` | Silent | Poison, shivs, agility, discard |
| `DefectCardPool` | Defect | Orbs, focus, lightning/frost/dark/plasma |
| `RegentCardPool` | Regent | Stances, mantras, divine/wrath/calm |
| `NecrobinderCardPool` | Necrobinder | Undead, summoning, death effects |
| `ColorlessCardPool` | All characters | Truly universal (no character affinity) |
| `CurseCardPool` | — | Curse cards only |
| `StatusCardPool` | — | Status cards only |

**Choose `ColorlessCardPool` ONLY if the card genuinely fits all characters equally.**
For any card with a thematic or mechanical affinity to a character, use that character's pool.

**If this card belongs to a NEW/CUSTOM mod character** (not one of the 5 above), that character
needs its own pool class. Name it after the character (e.g. character "Diaosi" → `DiaosiCardPool`).

```csharp
// File: YourMod/CardPools/DiaosiCardPool.cs
using BaseLib.Abstracts;
using Godot;
namespace YourMod.CardPools;

public class DiaosiCardPool : CustomCardPoolModel
{
    // Title: unique string ID (lowercase, no spaces)
    public override string Title => "diaosi";

    // IsColorless: false for character-specific pools
    public override bool IsColorless => false;

    // Card frame shape — must be one of the built-in material names:
    //   "card_frame_red"      (Ironclad)
    //   "card_frame_green"    (Silent)
    //   "card_frame_blue"     (Defect)
    //   "card_frame_orange"   (Regent)
    //   "card_frame_pink"     (Necrobinder)
    //   "card_frame_colorless", "card_frame_curse", "card_frame_quest"
    // Custom mod characters typically reuse one of these or use ShaderColor to recolor.
    public override string CardFrameMaterialPath => "card_frame_red";

    // ShaderColor: HSV recolors card_frame_red to any color.
    // Override this to give the character a distinct card color without needing a new material.
    // new Color("RRGGBBAA") in hex. Example: purple tint:
    public override Color ShaderColor => new Color("9B59B6FF");

    // DeckEntryCardColor: color dot shown next to the card in the deck list UI
    public override Color DeckEntryCardColor => new Color("9B59B6FF");
}
```

Then use `[Pool(typeof(DiaosiCardPool))]` on all cards for that character.
The character class must also have `public override CardPoolModel CardPool => ModelDb.CardPool<DiaosiCardPool>();`
If the mod already has a pool class defined, reuse it — do NOT create a duplicate.

### CRITICAL: [Pool] attribute is mandatory
- `CustomCardModel` has `autoAdd=true` by default — BaseLib auto-registers the card to its pool.
- But it MUST know WHICH pool via `[Pool(typeof(SomeCardPool))]`.
- **Missing [Pool] → game crashes on startup**: `Model X must be marked with a PoolAttribute`
- Do NOT create a Harmony patch to add cards to pools. BaseLib handles this automatically.

### Key enums
- `CardType`: Attack, Skill, Power, Status, Curse, Quest
- `CardRarity`: None, Basic, Common, Uncommon, Rare, Ancient, Event, Token, Status, Curse, Quest
- `TargetType`: None, Self, AnyEnemy, AllEnemies, RandomEnemy, AnyPlayer, AnyAlly, AllAllies, TargetedNoCreature, Osty

### StringExtensions.cs (image path helpers — check if already exists, add if not)
```csharp
namespace MyMod.Extensions;
public static class StringExtensions
{
    public static string CardImagePath(this string path) =>
        Path.Join(MainFile.ModId, "images", "card_portraits", path);

    public static string BigCardImagePath(this string path) =>
        Path.Join(MainFile.ModId, "images", "card_portraits", "big", path);
}
```

### Card Localization
Always create BOTH `eng/` and `zhs/` (Simplified Chinese) localization files.

**pack/MyMod/localization/eng/cards.json**
```json
{
  "MY_MOD-DARK_BLADE.title": "Dark Blade",
  "MY_MOD-DARK_BLADE.description": "Deal {Damage} damage.",
  "MY_MOD-DARK_BLADE.upgrade_description": "Deal {Damage} damage."
}
```

**pack/MyMod/localization/zhs/cards.json**
```json
{
  "MY_MOD-DARK_BLADE.title": "暗黑匕首",
  "MY_MOD-DARK_BLADE.description": "造成 {Damage} 点伤害。",
  "MY_MOD-DARK_BLADE.upgrade_description": "造成 {Damage} 点伤害。"
}
```

**CRITICAL: Use `{VarName}` placeholders, NOT hardcoded numbers.**
- `{Damage}` → live damage value (updates when Weakened/Strengthened)
- `{Block}` → live block value (updates when Frail/Dexterity)
- `{PoisonPower}` → live poison stacks from `PowerVar<PoisonPower>`
- Both `.description` and `.upgrade_description` use the same `{VarName}` — the engine shows the upgraded BaseValue automatically.
- Do NOT write `[red]8[/red]` with a literal number if a DynamicVar exists for it.
Key format: `{Prefix}-{Slugify(ClassName)}.suffix`
- Prefix = `t.Namespace.Substring(0, firstDot).ToUpperInvariant()` — simple uppercase of root namespace, NO CamelCase splitting
  - `S08_FuseCharge.Cards` → root `S08_FuseCharge` → ToUpperInvariant → `S08_FUSECHARGE` (NOT `S08_FUSE_CHARGE`)
  - `S09_Arcanist.Cards` → root `S09_Arcanist` → `S09_ARCANIST`
- ClassName: Slugify (CamelCase → UPPER_SNAKE_CASE): `FuseCharge` → `FUSE_CHARGE`
- Suffix: `.title` for card name, `.description`, `.upgrade_description`
- Keys are identical across languages; only values differ.
- Example: class `FuseCharge` in namespace `S08_FuseCharge.Cards` → key `S08_FUSECHARGE-FUSE_CHARGE`
