## New Character (Full Architecture)

Creating a new playable character requires FIVE components:
1. `ArcanistCardPool : CustomCardPoolModel`    — card pool
2. `ArcanistRelicPool : CustomRelicPoolModel`  — relic pool
3. `ArcanistPotionPool : CustomPotionPoolModel`— potion pool
4. Cards (CustomCardModel) + Relics (CustomRelicModel)
5. `Arcanist : PlaceholderCharacterModel`      — character model

### PlaceholderCharacterModel (use this for placeholder art — reuses existing character assets)
```csharp
using BaseLib.Abstracts;    // PlaceholderCharacterModel
using Godot;
using MegaCrit.Sts2.Core.Entities.Characters;
using MegaCrit.Sts2.Core.Models;

namespace MyMod.Characters;

public sealed class Arcanist : PlaceholderCharacterModel
{
    // Reuses Ironclad's art for all visual assets (portrait, select screen bg, energy icon, etc.)
    public override string PlaceholderID => "ironclad";   // or "silent", "defect" etc.

    public override Color NameColor => new Color("8B5CF6");  // Godot hex color
    public override CharacterGender Gender => CharacterGender.Neutral;  // or Male / Female
    public override int StartingHp => 60;

    // Point to your own pool classes
    public override CardPoolModel   CardPool   => ModelDb.CardPool<ArcanistCardPool>();
    public override RelicPoolModel  RelicPool  => ModelDb.RelicPool<ArcanistRelicPool>();
    public override PotionPoolModel PotionPool => ModelDb.PotionPool<ArcanistPotionPool>();

    // Starting deck — use ModelDb.Card<T>() (canonical, read-only)
    public override IEnumerable<CardModel> StartingDeck =>
    [
        ModelDb.Card<ArcaneStrike>(),
        ModelDb.Card<ArcaneStrike>(),
        ModelDb.Card<ArcaneStrike>(),
        ModelDb.Card<ArcaneDefend>(),
        ModelDb.Card<ArcaneDefend>(),
    ];

    // Starting relics — use ModelDb.Relic<T>() (canonical)
    public override IReadOnlyList<RelicModel> StartingRelics =>
        [ModelDb.Relic<ArcaneOrb>()];

    // Attack VFX paths used by architect enemies (copy from Ironclad if unsure)
    public override List<string> GetArchitectAttackVfx()
        => ["vfx/vfx_attack_slash", "vfx/vfx_attack_blunt", "vfx/vfx_heavy_blunt"];
}
```

### Pool classes (minimal implementations)
```csharp
// CardPool — name after character (e.g. Arcanist → ArcanistCardPool)
// Must set Title, IsColorless, CardFrameMaterialPath, ShaderColor, DeckEntryCardColor.
// CardFrameMaterialPath must be one of the built-in material names:
//   "card_frame_red", "card_frame_green", "card_frame_blue", "card_frame_orange",
//   "card_frame_pink", "card_frame_colorless", "card_frame_curse", "card_frame_quest"
// Use ShaderColor to recolor card_frame_red to any unique color (HSV shader).
public class ArcanistCardPool : CustomCardPoolModel
{
    public override string Title => "arcanist";
    public override bool IsColorless => false;
    public override string CardFrameMaterialPath => "card_frame_red";  // frame shape
    public override Color ShaderColor => new Color("7D3FC8FF");        // purple recolor
    public override Color DeckEntryCardColor => new Color("7D3FC8FF"); // deck list dot color
}

// RelicPool
public class ArcanistRelicPool : CustomRelicPoolModel { }

// PotionPool — needs to point to a parent pool (SharedPotionPool recommended)
public class ArcanistPotionPool : CustomPotionPoolModel
{
    public override PotionPoolModel? ParentPool => ModelDb.PotionPool<SharedPotionPool>();
}
```

### ModelDb pool accessors (add to sts2_api_reference if missing)
```csharp
ModelDb.CardPool<T>()    where T : CardPoolModel
ModelDb.RelicPool<T>()   where T : RelicPoolModel
ModelDb.PotionPool<T>()  where T : PotionPoolModel
```

### Character localization — requires a THIRD json file: characters.json
```json
// localization/eng/characters.json
{
  "S09_ARCANIST-ARCANIST.name": "Arcanist",
  "S09_ARCANIST-ARCANIST.description": "A wielder of arcane energies."
}
```
All three files needed: `cards.json`, `relics.json`, `characters.json` (eng + zhs).

### Registration
Characters register via `[Pool]` attribute (same mechanism as relics/cards) — no manual `ModelDb.Inject()` call needed if `PlaceholderCharacterModel` is used.
