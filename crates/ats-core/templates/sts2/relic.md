## Relics

### Base class: CustomRelicModel (BaseLib wrapper — auto-registers to pool)
```csharp
using BaseLib.Abstracts;      // CustomRelicModel
using BaseLib.Utils;          // PoolAttribute
using MegaCrit.Sts2.Core.Entities.Relics;
using MegaCrit.Sts2.Core.Entities.Creatures;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Models.RelicPools;  // SharedRelicPool
using MegaCrit.Sts2.Core.Runs;               // IRunState

namespace MyMod.Relics;

[Pool(typeof(SharedRelicPool))]               // auto-adds to relic reward pool
public sealed class FangedGrimoire : CustomRelicModel
{
    public override RelicRarity Rarity => RelicRarity.Common;
    public override bool IsAllowed(IRunState runState) => true;

    // REQUIRED for combat hooks to fire
    public override bool ShouldReceiveCombatHooks => true;

    // Image paths (relative inside .pck — no leading slash)
    public override string PackedIconPath           => "MyMod/images/relics/fanged_grimoire.png";
    protected override string PackedIconOutlinePath => "MyMod/images/relics/fanged_grimoire_outline.png";
    protected override string BigIconPath           => "MyMod/images/relics/big/fanged_grimoire.png";

    // All hook methods are virtual Task (default returns Task.CompletedTask).
    // Full list of 60+ hooks in sts2_api_reference.md (AbstractModel section).
    public override Task AfterDamageGiven(
        PlayerChoiceContext choiceContext,
        Creature? dealer,
        DamageResult result,
        ValueProp props,
        Creature target,
        CardModel? cardSource)
    {
        if (dealer?.IsPlayer == true && result.TotalDamage > 0)
        {
            dealer.GainBlockInternal(2);
            Flash();   // plays the relic flash animation
        }
        return Task.CompletedTask;
    }
}
```

### Key enums
- `RelicRarity`: None, Starter, Common, Uncommon, Rare, Shop, Event, Ancient

### CRITICAL: `ShouldReceiveCombatHooks => true` — without this, all combat hooks silently never fire.
### `Flash()` — plays the relic glowing animation (always call when the relic triggers).

### Pool registration — NO Harmony patch needed
`CustomRelicModel` (BaseLib) calls `CustomContentDictionary.AddModel()` in its constructor.
Combined with `[Pool(typeof(SharedRelicPool))]`, the relic is automatically added to the
shared relic reward pool. **Do NOT write a RelicPoolPatch** — it's redundant and was a
common mistake in older guides.

### Relic Localization — CRITICAL (missing crashes the entire relic encyclopedia list)
Key naming: CamelCase class name → UPPER_SNAKE_CASE (e.g. `FangedGrimoire` → `FANGED_GRIMOIRE`)
Always create BOTH `eng/` and `zhs/` files.

**pack/MyMod/localization/eng/relics.json**
```json
{
  "FANGED_GRIMOIRE.title": "Fanged Grimoire",
  "FANGED_GRIMOIRE.description": "Whenever you deal damage, gain [blue]2[/blue] Block.",
  "FANGED_GRIMOIRE.flavor": "A tome that feeds on violence."
}
```

**pack/MyMod/localization/zhs/relics.json**
```json
{
  "FANGED_GRIMOIRE.title": "獠牙魔典",
  "FANGED_GRIMOIRE.description": "每当你造成伤害时，获得 [blue]2[/blue] 点格挡。",
  "FANGED_GRIMOIRE.flavor": "一本以暴力为食的典籍。"
}
```
ALL custom relics must have .title, .description, and optionally .flavor entries.
