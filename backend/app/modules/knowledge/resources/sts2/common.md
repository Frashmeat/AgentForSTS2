# STS2 (Slay the Spire 2) Mod Development Reference

## Build Command
**ALWAYS use `dotnet publish`**, NOT `dotnet build`.
- `dotnet publish` = compiles DLL + exports Godot PCK + deploys both to the game's mods folder.
- `dotnet build` = only compiles DLL, no PCK export. Game won't load the mod.
- Game requires BOTH `.dll` AND `.pck` in `STS2GamePath/mods/<ModName>/`.

## How the Game Loads Mods
- On startup, the game scans `STS2GamePath/mods/` for subdirectories.
- A mod is recognized when the subdirectory contains BOTH `<ModName>.dll` AND `<ModName>.pck`.
- `.pck` contains `mod_manifest.json` (mod identity) and all image/localization assets.
- `.pck` format is version-locked to the Godot engine version: must use **Godot 4.5.1 Mono**.
  Using a newer Godot produces "Pack created with a newer version of the engine" error.

## Project Structure
```
MyMod/
├── local.props              ← local machine paths (NOT committed to git)
├── MyMod.csproj             ← project file
├── MainFile.cs              ← (or ModEntry.cs) mod entry point
├── Cards/
│   └── DarkBlade.cs
├── Relics/
│   └── MyRelic.cs
├── Extensions/
│   └── StringExtensions.cs  ← image path helpers
└── <ModName>/               ← Godot resource root (SAME NAME as mod)
    ├── mod_image.png
    ├── images/
    │   ├── card_portraits/
    │   │   ├── DarkBlade.png      (250×190)
    │   │   └── big/
    │   │       └── DarkBlade.png  (1000×760)
    │   └── relics/
    │       ├── my_relic.png           (icon)
    │       ├── my_relic_outline.png   (icon outline)
    │       └── big/
    │           └── my_relic.png       (large icon)
    └── localization/
        ├── eng/
        │   ├── relics.json
        │   └── cards.json
        └── zhs/              ← Simplified Chinese (optional, falls back to eng if absent)
            ├── relics.json
            └── cards.json
```

NOTE: The Godot resource directory is `<ModName>/<ModName>/...` — the mod name appears TWICE in the path.

## Localization Language Codes
Game-supported 3-letter codes: `eng`, `zhs` (Simplified Chinese), `deu`, `esp`, `fra`, `ita`, `jpn`, `kor`, `pol`, `ptb`, `rus`, `spa`, `tha`, `tur`.
- Mod path loaded by game: `res://{mod_id}/localization/{language}/{table}.json`
- If a language folder is absent from a mod, the game **automatically falls back to `eng/`**.
- Always provide at least `eng/`. Add `zhs/` for Chinese players.
Images must be placed under `project_root/<ModName>/images/...`, NOT `project_root/images/...`.

## local.props (required, never commit)
```xml
<Project>
  <PropertyGroup>
    <STS2GamePath>C:/SteamLibrary/.../Slay the Spire 2</STS2GamePath>
    <GodotExePath>C:/tools/Godot_v4.5.1-stable_mono_win64.exe</GodotExePath>
  </PropertyGroup>
</Project>
```

## Mod Entry Point
```csharp
[ModInitializer("Initialize")]
public class MainFile   // (some templates use ModEntry — read the existing file name)
{
    public static string ModId => "MyMod";   // must match assembly/folder name

    public static void Initialize()
    {
        var harmony = new Harmony("com.yourname.mymod");
        harmony.PatchAll();   // auto-applies all [HarmonyPatch] classes in assembly
    }
}
```

## Harmony Patching
```csharp
using HarmonyLib;

// In Initialize():
var harmony = new Harmony("com.yourname.modname");
harmony.PatchAll();  // finds all [HarmonyPatch] classes automatically

// Patch example:
[HarmonyPatch(typeof(SomeGameClass), "MethodName")]
public class MyPatch
{
    // Prefix: runs before the original method
    static bool Prefix(SomeGameClass __instance, ref int __result) { return true; }
    // Postfix: runs after the original method
    static void Postfix(SomeGameClass __instance, ref int __result) { }
}
```

## Common Gotchas Summary
1. `[Pool]` missing on card → crash: `must be marked with a PoolAttribute`
2. `ShouldReceiveCombatHooks` missing on relic → combat hooks silently don't fire
3. Relic localization missing → entire relic encyclopedia list appears empty (vanilla + mod)
4. `dotnet build` instead of `dotnet publish` → DLL only, PCK not updated, mod not loaded
5. Godot version != 4.5.1 → `Pack created with a newer version` error
6. Image placed at `project_root/images/...` instead of `project_root/<ModName>/images/...` → black card art
7. Game running during build → DLL locked, build fails
8. Card constructor uses `baseCost:` named parameter (NOT `cost:`) → compile error CS1739
9. Upgrade check: `IsUpgraded` (bool property), NOT `UpgradeLevel` (int) → compile error or wrong behavior
10. `new XxxModel()` → `DuplicateModelException` → black screen on run start; always use `ModelDb.Relic<T>().ToMutable()` etc.
11. Localization JSON must be **flat key-value**, NOT nested objects. `{"FOO.title": "Bar"}` ✅  `{"FOO": {"title": "Bar"}}` ❌ → `LocException: token type 'StartObject' as a string` → game crashes on startup.
    Localization text must NOT contain square brackets `[]` — the game's rich text renderer parses them as BBCode tags. `[MyTag]` text becomes an unclosed BBCode open tag → rendering exception → event state machine hangs, game gets stuck. Use `()`, `{}`, or plain text instead. `{"FOO.title": "Bar"}` ✅  `{"FOO": {"title": "Bar"}}` ❌ → `LocException: token type 'StartObject' as a string` → game crashes on startup.
12. `FromSimpleGridForRewards` / `FromSimpleGrid` work OUTSIDE combat (in events, Neow callbacks). The `CombatManager.IsEnding` check only early-returns during combat teardown, not a requirement for combat to exist. BrainLeech event uses this in a non-combat context.
13. `CardSelectCmd.FromChooseACardScreen` max 3 cards only — passing >3 throws ArgumentException. For showing many cards use `FromSimpleGridForRewards` instead.
14. Card localization key format is `{RootNS.ToUpperInvariant()}-{Slugify(ClassName)}.title`. The prefix is simple `.ToUpperInvariant()` (NO CamelCase splitting via Slugify). Example: `S08_FuseCharge.Cards` → prefix `S08_FUSECHARGE-`, class `FuseCharge` → `FUSE_CHARGE` → full key `S08_FUSECHARGE-FUSE_CHARGE.title`. Wrong: `S08_FUSE_CHARGE-FUSE_CHARGE` ❌  Wrong: `FUSE_CHARGE.title` ❌  Wrong: `.name` suffix ❌
15. Custom Neow/event option callbacks MUST call `SetEventFinished(LocString)` at the end, or the event state machine never advances — player sees no "Continue" button and is permanently stuck. `SetEventFinished` is `protected` on `EventModel`, so call via reflection:
    ```csharp
    typeof(EventModel).GetMethod("SetEventFinished",
        BindingFlags.NonPublic | BindingFlags.Instance)
        ?.Invoke(eventInstance, new object[] { new LocString("relics", "MY_KEY.title") });
    ```

## Built-in powers (MegaCrit.Sts2.Core.Models.Powers)
Do NOT redefine any of these — use them directly.
`using MegaCrit.Sts2.Core.Models.Powers;`

```csharp
// Generic pattern (same for all powers):
await PowerCmd.Apply<TPower>(target, amount, ownerCreature, cardSource);
bool has = creature.HasPower<TPower>();
int stacks = (int)creature.GetPower<TPower>().Amount;
```

**Common debuffs (Debuff | Counter unless noted):**
| Class | Effect |
|---|---|
| `PoisonPower` | Loses HP = stack count at turn end, then decrements |
| `VulnerablePower` | Takes 50% more damage |
| `WeakPower` | Deals 25% less damage |
| `FrailPower` | Gains 25% less block |
| `SlowPower` | Loses energy or slows action |
| `ConstrictPower` | Constriction damage per turn |
| `NoBlockPower` | Cannot gain block |

**Common buffs (Buff | Counter unless noted):**
| Class | Effect |
|---|---|
| `StrengthPower` | +N damage on attacks (can be negative) |
| `DexterityPower` | +N block on block cards (can be negative) |
| `ThornsPower` | Deals N damage when attacked |
| `RegenPower` | Heals N HP at turn end, then decrements |
| `ArtifactPower` | Negates next N debuffs |
| `IntangiblePower` | All damage reduced to 1 per hit |
| `PlatingPower` | Permanent block that persists between turns |
| `BarricadePower` | Block is never cleared at turn end (Buff | Single) |
| `BlurPower` | Block is not cleared at turn end this turn |
| `VigorPower` | Next attack deals +N damage (consumed on use) |
| `RagePower` | Gain N block each time a card is played |
| `EnragePower` | Gain N strength each time a skill is played |
| `LethalityPower` | Applies extra poison when poison is applied |
| `FocusPower` | +N to orb/passive effect power (Defect) |
| `RitualPower` | Gains N strength at turn end |
| `EnvenomPower` | Applies N poison when unblocked damage is dealt |
| `RupturePower` | Gains N strength when HP is lost from a card |
| `FeelNoPainPower` | Gains N block when a card is exhausted |
| `DarkEmbracePower` | Draws N cards when a card is exhausted |
| `NightmarePower` | Creates N copies of target card in hand next turn |
| `EchoFormPower` | First card each turn is played twice |
| `DuplicationPower` | Next card is played twice |
| `BurstPower` | Next N skills are played twice |
| `CorruptionPower` | Skills cost 0 but Exhaust (Buff | Single) |
| `CurlUpPower` | Gains N block when HP is reduced below threshold |
| `MayhemPower` | Auto-plays top card of draw pile at turn start |

## Debugging
Game logs: `%AppData%/SlayTheSpire2/logs/godot.log`
Key search terms: `[ERROR]`, `Finished mod initialization for` (confirms mod loaded)
