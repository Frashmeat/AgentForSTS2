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

// PotionPool — BaseLib 3.3.8 registers owned potions through [Pool].
// CustomPotionPoolModel has no ParentPool override in this version.
public class ArcanistPotionPool : CustomPotionPoolModel { }
```

### ModelDb pool accessors (add to sts2_api_reference if missing)
```csharp
ModelDb.CardPool<T>()    where T : CardPoolModel
ModelDb.RelicPool<T>()   where T : RelicPoolModel
ModelDb.PotionPool<T>()  where T : PotionPoolModel
```

### Character localization — characters.json plus Architect dialogue
```json
// localization/eng/characters.json
{
  "MYMODCHARACTERS-ARCANIST.title": "The Arcanist",
  "MYMODCHARACTERS-ARCANIST.titleObject": "the Arcanist",
  "MYMODCHARACTERS-ARCANIST.description": "A wielder of arcane energies.",
  "MYMODCHARACTERS-ARCANIST.pronounObject": "them",
  "MYMODCHARACTERS-ARCANIST.possessiveAdjective": "their",
  "MYMODCHARACTERS-ARCANIST.pronounPossessive": "theirs",
  "MYMODCHARACTERS-ARCANIST.pronounSubject": "they",
  "MYMODCHARACTERS-ARCANIST.goldMonologue": "Power has a price.",
  "MYMODCHARACTERS-ARCANIST.eventDeathPrevention": "Not yet.",
  "MYMODCHARACTERS-ARCANIST.aromaPrinciple": "Arcane ozone.",
  "MYMODCHARACTERS-ARCANIST.cardsModifierTitle": "Arcanist cards",
  "MYMODCHARACTERS-ARCANIST.cardsModifierDescription": "Arcanist cards now appear in rewards and shops.",
  "MYMODCHARACTERS-ARCANIST.banter.alive.endTurnPing": "Ready.",
  "MYMODCHARACTERS-ARCANIST.banter.dead.endTurnPing": "..."
}
```

`localization/<locale>/ancients.json` also requires the four
`THE_ARCHITECT.talk.MYMODCHARACTERS-ARCANIST.*` keys reported by the STS2 analyzer.
Cards require `title/description`, Relics require `title/description/flavor`, and all generated
localization tables must exist for both `eng` and `zhs`.

### Registration
`CustomCharacterModel` registers itself through `CustomContentDictionary.AddCharacter`.
Do not put `[Pool]` on a Character. `[Pool]` is only for Card, Relic, and Potion membership in the
Character's custom pool.
