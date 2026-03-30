## Custom Code / Mechanics

For effects that don't fit card/relic/power:
- Typical approach: Harmony Prefix/Postfix patches + optional per-player state in a static dictionary keyed by Player instance.
- For buff counters with icons: use PowerModel (see Power section).

### Hook availability
Any class inheriting `AbstractModel` (RelicModel, CardModel, PowerModel, etc.) automatically
gets 60+ combat hooks via override. No Harmony patches needed for hooks that already exist.
Key Task hooks (full list in sts2_api_reference.md):
  AfterCardPlayed, AfterDamageReceived, AfterDamageGiven, AfterPlayerTurnStart,
  AfterTurnEnd, BeforeAttack, AfterAttack, AfterCombatEnd, AfterBlockGained, ...

### Key PileType values
None, Draw, Hand, Discard, Exhaust, Play, Deck

### Neow event injection (proven pattern — S00_DebugPicker / SFTestKit)
To add an option to the Neow opening event, Harmony-patch `Neow.GenerateInitialOptions`:

```csharp
using MegaCrit.Sts2.Core.Events;        // EventOption, IHoverTip
using MegaCrit.Sts2.Core.Models.Events; // Neow
using MegaCrit.Sts2.Core.Entities.Players;
using HarmonyLib;

[HarmonyPatch(typeof(Neow), "GenerateInitialOptions")]
public static class MyNeowPatch
{
    static void Postfix(Neow __instance, ref IReadOnlyList<EventOption> __result)
    {
        Player? player = __instance.Owner;
        if (player == null) return;

        var option = new EventOption(
            __instance,
            () => MyAction(player),          // async Task callback
            title: new MegaCrit.Sts2.Core.Localization.LocString("relics", "MY_KEY.title"),
            description: new MegaCrit.Sts2.Core.Localization.LocString("relics", "MY_KEY.description"),
            textKey: "my_key",
            hoverTips: Array.Empty<IHoverTip>()
        );
        var list = __result.ToList();
        list.Add(option);
        __result = list.AsReadOnly();
    }

    private static async Task MyAction(Player player)
    {
        // --- Card grid selection (shows ALL cards in scrollable grid, pick any number) ---
        var allCards = ModelDb.AllCards
            .Select(m => new CardCreationResult(player.RunState.CreateCard(m, player)))
            .ToList();
        var prefs = new CardSelectorPrefs(
            CardSelectorPrefs.UpgradeSelectionPrompt, 0, allCards.Count);
        var selected = (await CardSelectCmd.FromSimpleGridForRewards(
            new BlockingPlayerChoiceContext(), allCards, player, prefs)).ToList();
        foreach (var card in selected)
            CardCmd.PreviewCardPileAdd(await CardPileCmd.Add(card, PileType.Deck));

        // --- Relic selection (full scrollable list) ---
        var allRelics = ModelDb.AllRelics
            .Where(r => r.Rarity is not RelicRarity.Starter and not RelicRarity.None)
            .Select(r => r.ToMutable()).ToList();
        var picked = await RelicSelectCmd.FromChooseARelicScreen(player, allRelics);
        if (picked != null) player.AddRelicInternal(picked);
    }
}
```

**Localization for Neow option** (in `<ModName>/localization/eng/relics.json`):
```json
{"MY_KEY": {"title": "Option Label", "description": "What this option does."}}
```
Note: Neow options use the `"relics"` localization table regardless of content.
