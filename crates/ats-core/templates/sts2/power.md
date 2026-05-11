## Powers (Buffs / Debuffs)

### IMPORTANT: Base class is PowerModel directly — there is NO CustomPowerModel
Unlike relics (CustomRelicModel), powers extend `PowerModel` from sts2.dll directly.
Do NOT search for CustomPowerModel — it does not exist.

### Canonical example: EnragePower (triggers on Skill cards, applies Strength)
```csharp
using System.Threading.Tasks;
using MegaCrit.Sts2.Core.Commands;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.Entities.Powers;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;

namespace MyMod.Powers;

public sealed class MyBuff : PowerModel
{
    // REQUIRED abstract properties
    public override PowerType Type => PowerType.Buff;            // or Debuff
    public override PowerStackType StackType => PowerStackType.Counter; // or Single

    // Trigger on Attack cards played:
    public override async Task AfterCardPlayed(PlayerChoiceContext context, CardPlay cardPlay)
    {
        if (cardPlay.Card.Type == CardType.Attack)  // Attack | Skill | Power | Status | Curse
        {
            // Deal extra damage to target using CreatureCmd
            await CreatureCmd.Damage(context, cardPlay.Target, Amount, ValueProp.Unpowered, Owner, null);
        }
        // No base call needed unless you want other powers to chain
    }

    // Duration-style: tick down each turn end, auto-removed when Amount reaches 0
    public override async Task AfterTurnEnd(PlayerChoiceContext choiceContext, CombatSide side)
    {
        await PowerCmd.TickDownDuration(this);  // decrements by 1, removes if 0
    }
}
```

### Canonical example: JuggernautPower (triggers on Block, damages random enemy)
```csharp
public override async Task AfterBlockGained(Creature creature, decimal amount, ValueProp props, CardModel? cardSource)
{
    if (amount > 0m && creature == Owner)
    {
        var enemies = CombatState.HittableEnemies;
        if (enemies.Count > 0)
        {
            Creature target = Owner.Player.RunState.Rng.CombatTargets.NextItem(enemies);
            Flash();  // visual flash on relic/power icon
            await CreatureCmd.Damage(new ThrowingPlayerChoiceContext(), target, Amount, ValueProp.Unpowered, Owner, null);
        }
    }
}
```

### Applying this power from a Card or Relic
```csharp
// Apply 3 stacks of MyBuff to the player (Owner is the creature receiving it)
await PowerCmd.Apply<MyBuff>(target, 3m, applier, cardSource);

// Tick down a power's duration (decrements by 1, removes at 0)
await PowerCmd.TickDownDuration(this);

// Remove a power immediately
await PowerCmd.Remove(this);
```

### Key enums
- `PowerType`: None, Buff, Debuff  (MegaCrit.Sts2.Core.Entities.Powers)
- `PowerStackType`: None, Counter, Single  (same namespace)
- `CardType`: Attack, Skill, Power, Status, Curse  (MegaCrit.Sts2.Core.Entities.Cards)

### Power image paths (raw PNG, NOT atlas)
```
<ModName>/images/powers/<PowerName>.png       (small icon, e.g. 48×48)
<ModName>/images/powers/big/<PowerName>.png   (large, e.g. 192×192)
```
Use these as the icon path string in your Power's image registration, same format as relics.

### Power Localization
Table name: `"powers"`. Keys: `<id>.title`, `<id>.description`.
Create both `eng/powers.json` and `zhs/powers.json`.
```json
{ "BlazePower": { "title": "Blaze", "description": "At the start of your turn, deal !A! damage." } }
```

### Full PowerModel hooks → see sts2_api_reference.md
Common hooks: AfterCardPlayed, AfterBlockGained, AfterTurnEnd, AfterDamageGiven,
AfterDamageReceived, AfterCombatEnd, BeforeAttack, AfterAttack
