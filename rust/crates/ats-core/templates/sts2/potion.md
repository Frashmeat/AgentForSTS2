## Potions

### IMPORTANT: CustomPotionModel is in BaseLib — NOT in decompiled_tmp
Do NOT search decompiled_tmp for CustomPotionModel. It lives in BaseLib.
Source: https://github.com/Alchyr/BaseLib-StS2/blob/master/Abstracts/CustomPotionModel.cs

### Base class: CustomPotionModel
```csharp
using BaseLib.Abstracts;        // CustomPotionModel
using BaseLib.Utils;            // PoolAttribute
using MegaCrit.Sts2.Core.Commands;          // PowerCmd, CreatureCmd
using MegaCrit.Sts2.Core.Entities.Cards;   // TargetType
using MegaCrit.Sts2.Core.Entities.Creatures;
using MegaCrit.Sts2.Core.Entities.Potions; // PotionRarity, PotionUsage
using MegaCrit.Sts2.Core.GameActions.Multiplayer;
using MegaCrit.Sts2.Core.Models.Powers;    // StrengthPower etc.
using MegaCrit.Sts2.Core.Models.PotionPools; // SharedPotionPool
using MegaCrit.Sts2.Core.ValueProps;

namespace MyMod.Potions;

[Pool(typeof(SharedPotionPool))]            // appears in the potion reward pool
public class BerserkerBrew : CustomPotionModel
{
    public override PotionRarity Rarity   => PotionRarity.Common;
    public override PotionUsage  Usage    => PotionUsage.CombatOnly;  // or AnyTime
    public override TargetType   TargetType => TargetType.Self;        // or AnyEnemy

    // Conditional use (property, NOT a method) — return false to grey out
    public override bool PassesCustomUsabilityCheck => Owner.Creature.CurrentHp > 8;

    // Effect method — override OnUse (NOT Use, NOT OnApply)
    protected override async Task OnUse(PlayerChoiceContext choiceContext, Creature? target)
    {
        // Apply 3 Strength to self
        await PowerCmd.Apply<StrengthPower>(Owner.Creature, 3m, Owner.Creature, null);

        // Deal 8 unblockable damage to self (bypasses block)
        await CreatureCmd.Damage(choiceContext, Owner.Creature, 8m, ValueProp.Unblockable, null, null);
    }
}
```

### Potion localization key format
BaseLib derives the loc key prefix from the root namespace via `TypePrefix.GetPrefix(type)`.
Pattern: `{NAMESPACE_ROOT}-{CLASS_NAME}` in UPPER_SNAKE_CASE.
Example: namespace `S07_BerserkerBrew.Potions` → class `BerserkerBrew` → key prefix `S07_BERSERKERBREW-BERSERKER_BREW`

**localization/eng/potions.json**
```json
{
  "S07_BERSERKERBREW-BERSERKER_BREW.title": "Berserker Brew",
  "S07_BERSERKERBREW-BERSERKER_BREW.description": "Gain [blue]3[/blue] Strength. Lose [red]8[/red] HP."
}
```
Also create `zhs/potions.json` with the same keys.

### Potion image path
`<ModName>/images/potions/<ClassName>.png`  (single image, no separate big/ variant needed)

### Key enums
- `PotionRarity`: Common, Uncommon, Rare
- `PotionUsage`: CombatOnly, AnyTime, OutsideCombat
