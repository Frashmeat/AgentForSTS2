use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use ats_game_context::{
    BehaviorAdapterError, BehaviorAdapterIdentity, BehaviorItemContext, BehaviorProposal,
    BehaviorRenderContext, CapabilityInvocation, CapabilityValue, GameBehaviorAdapter,
    RenderedFile, RenderedFileMerge, RenderedFileMergeKeyPolicy, RenderedItemBundle,
};
use ats_kernel::{BehaviorAdapterId, LocalizationFieldId, SchemaVersion, Sha256Digest};
use sha2::{Digest, Sha256};

pub const BEHAVIOR_ADAPTER_ID: &str = "game.sts2.behavior";
pub const BEHAVIOR_ADAPTER_IMPLEMENTATION_SHA256: &str =
    "d65f5f1472d4641547e08b04447850c99a61ba8c8497b8dfe8d1276000d75f52";

const CARD_DEAL_DAMAGE: &str = "card.on_play.deal_damage";
const CARD_GAIN_BLOCK: &str = "card.on_play.gain_block";
const CARD_DRAW_CARDS: &str = "card.on_play.draw_cards";
const CARD_GAIN_ENERGY: &str = "card.on_play.gain_energy";
const RELIC_COMBAT_START_GAIN_BLOCK: &str = "relic.combat_start.gain_block";
const POTION_ON_USE_GAIN_BLOCK: &str = "potion.on_use.gain_block";
const POWER_TURN_START_GAIN_BLOCK: &str = "power.turn_start.gain_block";

pub struct Sts2BehaviorAdapter {
    identity: BehaviorAdapterIdentity,
}

impl Sts2BehaviorAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            identity: BehaviorAdapterIdentity {
                id: BehaviorAdapterId::parse(BEHAVIOR_ADAPTER_ID)
                    .expect("built-in STS2 behavior Adapter ID is valid"),
                version: SchemaVersion::new(1).expect("built-in STS2 Adapter version is valid"),
                implementation_sha256: Sha256Digest::parse(BEHAVIOR_ADAPTER_IMPLEMENTATION_SHA256)
                    .expect("built-in STS2 Adapter implementation hash is valid"),
            },
        }
    }
}

impl Default for Sts2BehaviorAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameBehaviorAdapter for Sts2BehaviorAdapter {
    fn identity(&self) -> &BehaviorAdapterIdentity {
        &self.identity
    }

    fn validate_ir(
        &self,
        context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<(), BehaviorAdapterError> {
        if context.game_pack_id.as_str() != "sts2" || proposal.adapter != self.identity {
            return Err(BehaviorAdapterError::InvalidContext);
        }

        match proposal.item_type.as_str() {
            "character" => validate_character(context, proposal),
            "card" => validate_card(context, proposal),
            "relic" => validate_relic(context, proposal),
            "potion" => validate_potion(context, proposal),
            "power" => validate_power(context, proposal),
            _ => Err(BehaviorAdapterError::UnsupportedCapability),
        }
    }

    fn render(
        &self,
        context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<RenderedItemBundle, BehaviorAdapterError> {
        self.validate_ir(context, proposal)?;
        let files = match proposal.item_type.as_str() {
            "character" => render_character(context)?,
            "card" => render_card(context, proposal)?,
            "relic" => render_relic(context, proposal)?,
            "potion" => render_potion(context, proposal)?,
            "power" => render_power(context, proposal)?,
            _ => return Err(BehaviorAdapterError::UnsupportedCapability),
        };
        RenderedItemBundle::new(proposal, files)
    }
}

fn validate_character(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<(), BehaviorAdapterError> {
    if !proposal.invocations.is_empty()
        || !matches!(
            choice(&context.item, "visual_profile")?,
            "placeholder" | "branded_placeholder"
        )
        || !matches!(
            choice(&context.item, "placeholder_id")?,
            "ironclad" | "silent" | "defect" | "regent" | "necrobinder"
        )
        || !matches!(
            choice(&context.item, "gender")?,
            "male" | "female" | "neutral"
        )
        || !(1..=999).contains(&integer(&context.item, "starting_hp")?)
        || !(0..=999_999).contains(&integer(&context.item, "starting_gold")?)
        || !(0..=99).contains(&integer(&context.item, "max_energy")?)
        || !valid_hex_color(text(&context.item, "name_color")?)
    {
        return Err(BehaviorAdapterError::InvalidIr);
    }
    require_localizations(&context.item, CHARACTER_LOCALIZATION_FIELDS)?;
    require_reference_types(&context.item, "starting_deck", "card")?;
    require_reference_types(&context.item, "cards", "card")?;
    require_reference_types(&context.item, "starting_relics", "relic")?;
    require_reference_types(&context.item, "relics", "relic")?;
    require_optional_reference_types(&context.item, "potions", "potion")?;
    require_optional_reference_types(&context.item, "powers", "power")?;
    if choice(&context.item, "visual_profile")? == "branded_placeholder" {
        require_paths(
            &context.item,
            &[
                "character.map_marker",
                "character.select_icon",
                "character.select_locked_icon",
                "character.top_panel_icon",
                "character.top_panel_icon_outline",
            ],
        )?;
    }
    Ok(())
}

fn validate_card(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<(), BehaviorAdapterError> {
    validate_invocations(
        proposal,
        &[
            CARD_DEAL_DAMAGE,
            CARD_GAIN_BLOCK,
            CARD_DRAW_CARDS,
            CARD_GAIN_ENERGY,
        ],
    )?;
    let pool = choice(&context.item, "pool")?;
    if !matches!(
        pool,
        "ironclad"
            | "silent"
            | "defect"
            | "regent"
            | "necrobinder"
            | "colorless"
            | "curse"
            | "status"
            | "custom_character"
    ) || !matches!(
        choice(&context.item, "card_type")?,
        "attack" | "skill" | "power" | "status" | "curse" | "quest"
    ) || !matches!(
        choice(&context.item, "rarity")?,
        "none"
            | "basic"
            | "common"
            | "uncommon"
            | "rare"
            | "ancient"
            | "event"
            | "token"
            | "status"
            | "curse"
            | "quest"
    ) || !matches!(
        choice(&context.item, "target")?,
        "none"
            | "self"
            | "any_enemy"
            | "all_enemies"
            | "random_enemy"
            | "any_player"
            | "any_ally"
            | "all_allies"
            | "targeted_no_creature"
            | "osty"
    ) || !(-1..=9).contains(&integer(&context.item, "base_cost")?)
    {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    for invocation in &proposal.invocations {
        let amount = amount(invocation)?;
        if amount <= 0 {
            return Err(BehaviorAdapterError::InvalidIr);
        }
        if invocation.capability_id.as_str() == CARD_DEAL_DAMAGE
            && !matches!(
                choice(&context.item, "target")?,
                "any_enemy" | "all_enemies" | "random_enemy"
            )
        {
            return Err(BehaviorAdapterError::InvalidIr);
        }
    }
    if pool == "custom_character" {
        require_single_reference(&context.item, "owner_character", "character")?;
    }
    require_localizations(&context.item, &["name", "description"])?;
    require_paths(&context.item, &["card.portrait", "card.big"])
}

fn validate_relic(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<(), BehaviorAdapterError> {
    if !matches!(
        choice(&context.item, "rarity")?,
        "starter" | "common" | "uncommon" | "rare" | "shop" | "event" | "ancient"
    ) {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    validate_invocations(proposal, &[RELIC_COMBAT_START_GAIN_BLOCK])?;
    require_positive_amounts(proposal)?;
    require_localizations(&context.item, &["name", "description"])?;
    require_paths(
        &context.item,
        &["relic.normal", "relic.outline", "relic.big"],
    )?;
    require_single_reference(&context.item, "owner_character", "character")?;
    Ok(())
}

fn validate_potion(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<(), BehaviorAdapterError> {
    if !matches!(
        choice(&context.item, "rarity")?,
        "common" | "uncommon" | "rare"
    ) || !matches!(choice(&context.item, "usage")?, "combat_only" | "anytime")
        || !matches!(
            choice(&context.item, "target")?,
            "self" | "any_player" | "any_ally"
        )
    {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    validate_invocations(proposal, &[POTION_ON_USE_GAIN_BLOCK])?;
    require_positive_amounts(proposal)?;
    require_localizations(&context.item, &["name", "description"])?;
    require_paths(&context.item, &["potion.icon"])?;
    require_single_reference(&context.item, "owner_character", "character")?;
    Ok(())
}

fn validate_power(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<(), BehaviorAdapterError> {
    if !matches!(choice(&context.item, "power_type")?, "buff" | "debuff")
        || !matches!(
            choice(&context.item, "stack_type")?,
            "none" | "counter" | "single"
        )
        || !matches!(
            choice(&context.item, "instance_type")?,
            "none" | "per_source"
        )
        || boolean(&context.item, "allow_negative").is_err()
    {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    validate_invocations(proposal, &[POWER_TURN_START_GAIN_BLOCK])?;
    require_positive_amounts(proposal)?;
    require_localizations(&context.item, &["name", "description"])?;
    require_paths(&context.item, &["power.icon", "power.big"])
}

fn render_character(
    context: &BehaviorRenderContext,
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let namespace = namespace_for(&context.mod_id);
    let class = class_name("Character", &context.item.definition_hash);
    let card_pool = format!("{class}CardPool");
    let relic_pool = format!("{class}RelicPool");
    let potion_pool = format!("{class}PotionPool");
    let mut source = String::new();
    writeln!(source, "using System.Collections.Generic;").unwrap();
    writeln!(source, "using BaseLib.Abstracts;").unwrap();
    writeln!(source, "using Godot;").unwrap();
    writeln!(source, "using MegaCrit.Sts2.Core.Entities.Characters;").unwrap();
    writeln!(source, "using MegaCrit.Sts2.Core.Models;\n").unwrap();
    writeln!(source, "namespace {namespace};\n").unwrap();
    writeln!(
        source,
        "public sealed class {class} : PlaceholderCharacterModel\n{{"
    )
    .unwrap();
    writeln!(
        source,
        "    public override string PlaceholderID => {};",
        csharp_string(choice(&context.item, "placeholder_id")?)
    )
    .unwrap();
    writeln!(
        source,
        "    public override Color NameColor => new Color({});",
        csharp_string(text(&context.item, "name_color")?.trim_start_matches('#'))
    )
    .unwrap();
    writeln!(
        source,
        "    public override CharacterGender Gender => CharacterGender.{};",
        pascal(choice(&context.item, "gender")?)
    )
    .unwrap();
    writeln!(
        source,
        "    public override int StartingHp => {};",
        integer(&context.item, "starting_hp")?
    )
    .unwrap();
    writeln!(
        source,
        "    public override int StartingGold => {};",
        integer(&context.item, "starting_gold")?
    )
    .unwrap();
    writeln!(
        source,
        "    public override int MaxEnergy => {};",
        integer(&context.item, "max_energy")?
    )
    .unwrap();
    writeln!(
        source,
        "    public override CardPoolModel CardPool => ModelDb.CardPool<{card_pool}>();"
    )
    .unwrap();
    writeln!(
        source,
        "    public override RelicPoolModel RelicPool => ModelDb.RelicPool<{relic_pool}>();"
    )
    .unwrap();
    writeln!(
        source,
        "    public override PotionPoolModel PotionPool => ModelDb.PotionPool<{potion_pool}>();"
    )
    .unwrap();
    render_model_list(
        &mut source,
        "IEnumerable<CardModel>",
        "StartingDeck",
        "Card",
        references(&context.item, "starting_deck")?,
    )?;
    render_model_list(
        &mut source,
        "IReadOnlyList<RelicModel>",
        "StartingRelics",
        "Relic",
        references(&context.item, "starting_relics")?,
    )?;
    writeln!(source, "    public override List<string> GetArchitectAttackVfx() => [\"vfx/vfx_attack_slash\", \"vfx/vfx_attack_blunt\", \"vfx/vfx_heavy_blunt\"];").unwrap();
    if choice(&context.item, "visual_profile")? == "branded_placeholder" {
        writeln!(
            source,
            "    public override string CustomIconTexturePath => {};",
            csharp_string(path_for_role(&context.item, "character.top_panel_icon")?)
        )
        .unwrap();
        writeln!(
            source,
            "    public override string CustomIconOutlineTexturePath => {};",
            csharp_string(path_for_role(
                &context.item,
                "character.top_panel_icon_outline"
            )?)
        )
        .unwrap();
        writeln!(
            source,
            "    public override string CustomCharacterSelectIconPath => {};",
            csharp_string(path_for_role(&context.item, "character.select_icon")?)
        )
        .unwrap();
        writeln!(
            source,
            "    public override string CustomCharacterSelectLockedIconPath => {};",
            csharp_string(path_for_role(
                &context.item,
                "character.select_locked_icon"
            )?)
        )
        .unwrap();
        writeln!(
            source,
            "    public override string CustomMapMarkerPath => {};",
            csharp_string(path_for_role(&context.item, "character.map_marker")?)
        )
        .unwrap();
    }
    writeln!(source, "}}\n").unwrap();
    writeln!(
        source,
        "public sealed class {card_pool} : CustomCardPoolModel\n{{"
    )
    .unwrap();
    writeln!(
        source,
        "    public override string Title => {};",
        csharp_string(&class.to_lowercase())
    )
    .unwrap();
    writeln!(source, "    public override bool IsColorless => false;").unwrap();
    writeln!(
        source,
        "    public override Color DeckEntryCardColor => new Color({});",
        csharp_string(text(&context.item, "name_color")?.trim_start_matches('#'))
    )
    .unwrap();
    writeln!(source, "}}\npublic sealed class {relic_pool} : CustomRelicPoolModel {{ }}\npublic sealed class {potion_pool} : CustomPotionPoolModel {{ }}").unwrap();

    let mut files = vec![source_file(context, source)?];
    files.extend(localization_files(
        context,
        "characters",
        CHARACTER_LOCALIZATION_FIELDS,
    )?);
    files.extend(character_ancients_files(context)?);
    Ok(files)
}

fn render_card(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let namespace = namespace_for(&context.mod_id);
    let class = class_name("Card", &context.item.definition_hash);
    let pool = card_pool_type(&context.item)?;
    let mut vars = Vec::<String>::new();
    let mut effects = Vec::<String>::new();
    for invocation in &proposal.invocations {
        let amount = amount(invocation)?;
        match invocation.capability_id.as_str() {
            CARD_DEAL_DAMAGE => {
                vars.push(format!("new DamageVar({amount}m, ValueProp.Move)"));
                effects.push(match choice(&context.item, "target")? {
                    "any_enemy" => "        ArgumentNullException.ThrowIfNull(cardPlay.Target);\n        await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue).FromCard(this).Targeting(cardPlay.Target).Execute(choiceContext);".into(),
                    "all_enemies" => "        await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue).FromCard(this).TargetingAllOpponents(base.CombatState).Execute(choiceContext);".into(),
                    "random_enemy" => "        await DamageCmd.Attack(base.DynamicVars.Damage.BaseValue).FromCard(this).TargetingRandomOpponents(base.CombatState).Execute(choiceContext);".into(),
                    _ => return Err(BehaviorAdapterError::InvalidIr),
                });
            }
            CARD_GAIN_BLOCK => {
                vars.push(format!("new BlockVar({amount}m, ValueProp.Move)"));
                effects.push("        await CreatureCmd.GainBlock(base.Owner.Creature, base.DynamicVars.Block, cardPlay);".into());
            }
            CARD_DRAW_CARDS => {
                vars.push(format!("new CardsVar({amount})"));
                effects.push("        await CardPileCmd.Draw(choiceContext, base.DynamicVars.Cards.BaseValue, base.Owner);".into());
            }
            CARD_GAIN_ENERGY => {
                vars.push(format!("new EnergyVar({amount})"));
                effects.push("        await PlayerCmd.GainEnergy(base.DynamicVars.Energy.BaseValue, base.Owner);".into());
            }
            _ => return Err(BehaviorAdapterError::UnsupportedCapability),
        }
    }
    let source = format!(
        "using System;\nusing System.Collections.Generic;\nusing System.Threading.Tasks;\nusing BaseLib.Abstracts;\nusing BaseLib.Utils;\nusing MegaCrit.Sts2.Core.Commands;\nusing MegaCrit.Sts2.Core.Entities.Cards;\nusing MegaCrit.Sts2.Core.GameActions.Multiplayer;\nusing MegaCrit.Sts2.Core.Localization.DynamicVars;\nusing MegaCrit.Sts2.Core.Models.CardPools;\nusing MegaCrit.Sts2.Core.ValueProps;\n\nnamespace {namespace};\n\n[Pool(typeof({pool}))]\npublic sealed class {class}() : CustomCardModel(\n    baseCost: {cost},\n    type: CardType.{card_type},\n    rarity: CardRarity.{rarity},\n    target: TargetType.{target})\n{{\n    protected override IEnumerable<DynamicVar> CanonicalVars => [{vars}];\n    public override string PortraitPath => {portrait};\n    public override string CustomPortraitPath => {big};\n\n    protected override async Task OnPlay(PlayerChoiceContext choiceContext, CardPlay cardPlay)\n    {{\n{effects}\n    }}\n}}\n",
        cost = integer(&context.item, "base_cost")?,
        card_type = pascal(choice(&context.item, "card_type")?),
        rarity = pascal(choice(&context.item, "rarity")?),
        target = target_type(choice(&context.item, "target")?)?,
        vars = vars.join(", "),
        portrait = csharp_string(path_for_role(&context.item, "card.portrait")?),
        big = csharp_string(path_for_role(&context.item, "card.big")?),
        effects = effects.join("\n"),
    );
    let mut files = vec![source_file(context, source)?];
    files.extend(localization_files(
        context,
        "cards",
        &["name", "description"],
    )?);
    Ok(files)
}

fn render_relic(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let namespace = namespace_for(&context.mod_id);
    let class = class_name("Relic", &context.item.definition_hash);
    let owner = require_single_reference(&context.item, "owner_character", "character")?;
    let pool = format!(
        "{}RelicPool",
        class_name("Character", &owner.definition_hash)
    );
    let amount = amount(&proposal.invocations[0])?;
    let source = format!(
        "using System.Collections.Generic;\nusing System.Threading.Tasks;\nusing BaseLib.Abstracts;\nusing BaseLib.Utils;\nusing MegaCrit.Sts2.Core.Commands;\nusing MegaCrit.Sts2.Core.Entities.Relics;\nusing MegaCrit.Sts2.Core.Localization.DynamicVars;\nusing MegaCrit.Sts2.Core.ValueProps;\n\nnamespace {namespace};\n\n[Pool(typeof({pool}))]\npublic sealed class {class} : CustomRelicModel\n{{\n    public override RelicRarity Rarity => RelicRarity.{rarity};\n    protected override IEnumerable<DynamicVar> CanonicalVars => [new BlockVar({amount}m, ValueProp.Unpowered)];\n    public override string PackedIconPath => {normal};\n    protected override string PackedIconOutlinePath => {outline};\n    protected override string BigIconPath => {big};\n\n    public override async Task BeforeCombatStart()\n    {{\n        Flash();\n        await CreatureCmd.GainBlock(base.Owner.Creature, base.DynamicVars.Block, null);\n    }}\n}}\n",
        rarity = pascal(choice(&context.item, "rarity")?),
        normal = csharp_string(path_for_role(&context.item, "relic.normal")?),
        outline = csharp_string(path_for_role(&context.item, "relic.outline")?),
        big = csharp_string(path_for_role(&context.item, "relic.big")?),
    );
    let mut files = vec![source_file(context, source)?];
    files.extend(localization_files(
        context,
        "relics",
        &["name", "description"],
    )?);
    Ok(files)
}

fn render_potion(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let namespace = namespace_for(&context.mod_id);
    let class = class_name("Potion", &context.item.definition_hash);
    let owner = require_single_reference(&context.item, "owner_character", "character")?;
    let pool = format!(
        "{}PotionPool",
        class_name("Character", &owner.definition_hash)
    );
    let amount = amount(&proposal.invocations[0])?;
    let source = format!(
        "using System.Collections.Generic;\nusing System.Threading.Tasks;\nusing BaseLib.Abstracts;\nusing BaseLib.Utils;\nusing MegaCrit.Sts2.Core.Commands;\nusing MegaCrit.Sts2.Core.Entities.Cards;\nusing MegaCrit.Sts2.Core.Entities.Creatures;\nusing MegaCrit.Sts2.Core.Entities.Potions;\nusing MegaCrit.Sts2.Core.GameActions.Multiplayer;\nusing MegaCrit.Sts2.Core.Localization.DynamicVars;\nusing MegaCrit.Sts2.Core.ValueProps;\n\nnamespace {namespace};\n\n[Pool(typeof({pool}))]\npublic sealed class {class} : CustomPotionModel\n{{\n    public override PotionRarity Rarity => PotionRarity.{rarity};\n    public override PotionUsage Usage => PotionUsage.{usage};\n    public override TargetType TargetType => TargetType.{target};\n    protected override IEnumerable<DynamicVar> CanonicalVars => [new BlockVar({amount}m, ValueProp.Unpowered)];\n    public override string CustomPackedImagePath => {icon};\n\n    protected override async Task OnUse(PlayerChoiceContext choiceContext, Creature? target)\n    {{\n        Creature effectTarget = target ?? base.Owner.Creature;\n        await CreatureCmd.GainBlock(effectTarget, base.DynamicVars.Block, null);\n    }}\n}}\n",
        rarity = pascal(choice(&context.item, "rarity")?),
        usage = match choice(&context.item, "usage")? {
            "combat_only" => "CombatOnly",
            "anytime" => "Anytime",
            _ => return Err(BehaviorAdapterError::InvalidContext),
        },
        target = target_type(choice(&context.item, "target")?)?,
        icon = csharp_string(path_for_role(&context.item, "potion.icon")?),
    );
    let mut files = vec![source_file(context, source)?];
    files.extend(localization_files(
        context,
        "potions",
        &["name", "description"],
    )?);
    Ok(files)
}

fn render_power(
    context: &BehaviorRenderContext,
    proposal: &BehaviorProposal,
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let namespace = namespace_for(&context.mod_id);
    let class = class_name("Power", &context.item.definition_hash);
    let amount = amount(&proposal.invocations[0])?;
    let source = format!(
        "using System.Collections.Generic;\nusing System.Threading.Tasks;\nusing BaseLib.Abstracts;\nusing MegaCrit.Sts2.Core.Commands;\nusing MegaCrit.Sts2.Core.Entities.Players;\nusing MegaCrit.Sts2.Core.Entities.Powers;\nusing MegaCrit.Sts2.Core.GameActions.Multiplayer;\nusing MegaCrit.Sts2.Core.Localization.DynamicVars;\nusing MegaCrit.Sts2.Core.ValueProps;\n\nnamespace {namespace};\n\npublic sealed class {class} : CustomPowerModel\n{{\n    public override PowerType Type => PowerType.{power_type};\n    public override PowerStackType StackType => PowerStackType.{stack_type};\n    public override PowerInstanceType InstanceType => PowerInstanceType.{instance_type};\n    public override bool AllowNegative => {allow_negative};\n    protected override IEnumerable<DynamicVar> CanonicalVars => [new BlockVar({amount}m, ValueProp.Unpowered)];\n    public override string CustomPackedIconPath => {icon};\n    public override string CustomBigIconPath => {big};\n\n    public override async Task AfterPlayerTurnStart(PlayerChoiceContext choiceContext, Player player)\n    {{\n        if (player == base.Owner.Player)\n        {{\n            Flash();\n            await CreatureCmd.GainBlock(base.Owner, base.DynamicVars.Block, null);\n        }}\n    }}\n}}\n",
        power_type = pascal(choice(&context.item, "power_type")?),
        stack_type = pascal(choice(&context.item, "stack_type")?),
        instance_type = match choice(&context.item, "instance_type")? {
            "none" => "None",
            "per_source" => "PerSource",
            _ => return Err(BehaviorAdapterError::InvalidContext),
        },
        allow_negative = boolean(&context.item, "allow_negative")?,
        icon = csharp_string(path_for_role(&context.item, "power.icon")?),
        big = csharp_string(path_for_role(&context.item, "power.big")?),
    );
    let mut files = vec![source_file(context, source)?];
    files.extend(localization_files(
        context,
        "powers",
        &["name", "description"],
    )?);
    Ok(files)
}

fn source_file(
    context: &BehaviorRenderContext,
    source: String,
) -> Result<RenderedFile, BehaviorAdapterError> {
    RenderedFile::new(
        "source",
        format!("Generated/{}.cs", context.item.item_id.as_str()),
        source.into_bytes(),
    )
}

fn localization_files(
    context: &BehaviorRenderContext,
    table: &str,
    fields: &[&str],
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let model_key = model_key(context);
    let mut files = Vec::new();
    for (locale, localization) in &context.item.localizations {
        let mut values = BTreeMap::new();
        for field in fields {
            let value = localization_value(localization, field)?;
            let suffix = localization_suffix(table, field);
            values.insert(format!("{model_key}.{suffix}"), value.to_owned());
            if table == "cards" && *field == "description" {
                values.insert(format!("{model_key}.upgrade_description"), value.to_owned());
            }
            if table == "relics" && *field == "description" {
                values.insert(format!("{model_key}.flavor"), value.to_owned());
            }
        }
        files.push(
            RenderedFile::new(
                format!("localization.{}", locale.as_str()),
                format!(
                    "{}/localization/{}/{}.json",
                    context.mod_id,
                    locale.as_str(),
                    table
                ),
                serde_json::to_vec(&values).map_err(|_| BehaviorAdapterError::InvalidOutput)?,
            )?
            .with_composition_merge(
                RenderedFileMerge::JsonObject,
                RenderedFileMergeKeyPolicy::UniqueKeys,
            ),
        );
    }
    Ok(files)
}

fn character_ancients_files(
    context: &BehaviorRenderContext,
) -> Result<Vec<RenderedFile>, BehaviorAdapterError> {
    let model_key = model_key(context);
    let mut files = Vec::new();
    for (locale, localization) in &context.item.localizations {
        let mut values = BTreeMap::new();
        for (field, suffix) in [
            ("title", "0-0r.char"),
            ("title_object", "0-0r.next"),
            ("description", "0-1r.ancient"),
            ("cards_modifier_description", "0-attack"),
        ] {
            values.insert(
                format!("THE_ARCHITECT.talk.{model_key}.{suffix}"),
                localization_value(localization, field)?.to_owned(),
            );
        }
        files.push(
            RenderedFile::new(
                format!("localization.ancients.{}", locale.as_str()),
                format!(
                    "{}/localization/{}/ancients.json",
                    context.mod_id,
                    locale.as_str()
                ),
                serde_json::to_vec(&values).map_err(|_| BehaviorAdapterError::InvalidOutput)?,
            )?
            .with_composition_merge(
                RenderedFileMerge::JsonObject,
                RenderedFileMergeKeyPolicy::ExclusivePath,
            ),
        );
    }
    Ok(files)
}

fn render_model_list(
    source: &mut String,
    collection_type: &str,
    property: &str,
    accessor: &str,
    references: &[ats_game_context::BehaviorItemReference],
) -> Result<(), BehaviorAdapterError> {
    writeln!(
        source,
        "    public override {collection_type} {property} =>"
    )
    .unwrap();
    writeln!(source, "    [").unwrap();
    for reference in references {
        let class = class_name(
            &pascal(reference.item_type.as_str()),
            &reference.definition_hash,
        );
        for _ in 0..reference.quantity {
            writeln!(source, "        ModelDb.{accessor}<{class}>(),").unwrap();
        }
    }
    writeln!(source, "    ];").unwrap();
    Ok(())
}

fn card_pool_type(item: &BehaviorItemContext) -> Result<String, BehaviorAdapterError> {
    Ok(match choice(item, "pool")? {
        "ironclad" => "IroncladCardPool".into(),
        "silent" => "SilentCardPool".into(),
        "defect" => "DefectCardPool".into(),
        "regent" => "RegentCardPool".into(),
        "necrobinder" => "NecrobinderCardPool".into(),
        "colorless" => "ColorlessCardPool".into(),
        "curse" => "CurseCardPool".into(),
        "status" => "StatusCardPool".into(),
        "custom_character" => {
            let owner = require_single_reference(item, "owner_character", "character")?;
            format!(
                "{}CardPool",
                class_name("Character", &owner.definition_hash)
            )
        }
        _ => return Err(BehaviorAdapterError::InvalidContext),
    })
}

fn validate_invocations(
    proposal: &BehaviorProposal,
    allowed: &[&str],
) -> Result<(), BehaviorAdapterError> {
    let mut seen = BTreeSet::new();
    if proposal.invocations.is_empty()
        || proposal.invocations.iter().any(|invocation| {
            !allowed.contains(&invocation.capability_id.as_str())
                || !seen.insert(invocation.capability_id.as_str())
        })
    {
        return Err(BehaviorAdapterError::UnsupportedCapability);
    }
    Ok(())
}

fn require_positive_amounts(proposal: &BehaviorProposal) -> Result<(), BehaviorAdapterError> {
    for invocation in &proposal.invocations {
        if amount(invocation)? <= 0 {
            return Err(BehaviorAdapterError::InvalidIr);
        }
    }
    Ok(())
}

fn amount(invocation: &CapabilityInvocation) -> Result<i64, BehaviorAdapterError> {
    match invocation
        .arguments
        .iter()
        .find(|(id, _)| id.as_str() == "amount")
        .map(|(_, value)| value)
    {
        Some(CapabilityValue::Integer(value)) => Ok(*value),
        _ => Err(BehaviorAdapterError::InvalidIr),
    }
}

fn choice<'a>(item: &'a BehaviorItemContext, id: &str) -> Result<&'a str, BehaviorAdapterError> {
    match field(item, id)? {
        CapabilityValue::Choice(value) => Ok(value),
        _ => Err(BehaviorAdapterError::InvalidContext),
    }
}

fn integer(item: &BehaviorItemContext, id: &str) -> Result<i64, BehaviorAdapterError> {
    match field(item, id)? {
        CapabilityValue::Integer(value) => Ok(*value),
        _ => Err(BehaviorAdapterError::InvalidContext),
    }
}

fn boolean(item: &BehaviorItemContext, id: &str) -> Result<bool, BehaviorAdapterError> {
    match field(item, id)? {
        CapabilityValue::Boolean(value) => Ok(*value),
        _ => Err(BehaviorAdapterError::InvalidContext),
    }
}

fn text<'a>(item: &'a BehaviorItemContext, id: &str) -> Result<&'a str, BehaviorAdapterError> {
    match field(item, id)? {
        CapabilityValue::Text(value) => Ok(value),
        _ => Err(BehaviorAdapterError::InvalidContext),
    }
}

fn field<'a>(
    item: &'a BehaviorItemContext,
    id: &str,
) -> Result<&'a CapabilityValue, BehaviorAdapterError> {
    item.canonical_fields
        .iter()
        .find(|(field_id, _)| field_id.as_str() == id)
        .map(|(_, value)| value)
        .ok_or(BehaviorAdapterError::InvalidContext)
}

fn references<'a>(
    item: &'a BehaviorItemContext,
    slot: &str,
) -> Result<&'a [ats_game_context::BehaviorItemReference], BehaviorAdapterError> {
    item.references
        .iter()
        .find(|(slot_id, _)| slot_id.as_str() == slot)
        .map(|(_, values)| values.as_slice())
        .ok_or(BehaviorAdapterError::InvalidContext)
}

fn require_reference_types(
    item: &BehaviorItemContext,
    slot: &str,
    item_type: &str,
) -> Result<(), BehaviorAdapterError> {
    if references(item, slot)?
        .iter()
        .any(|reference| reference.item_type.as_str() != item_type)
    {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    Ok(())
}

fn require_optional_reference_types(
    item: &BehaviorItemContext,
    slot: &str,
    item_type: &str,
) -> Result<(), BehaviorAdapterError> {
    let Some(references) = item
        .references
        .iter()
        .find(|(slot_id, _)| slot_id.as_str() == slot)
    else {
        return Ok(());
    };
    if references
        .1
        .iter()
        .any(|reference| reference.item_type.as_str() != item_type)
    {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    Ok(())
}

fn require_single_reference<'a>(
    item: &'a BehaviorItemContext,
    slot: &str,
    item_type: &str,
) -> Result<&'a ats_game_context::BehaviorItemReference, BehaviorAdapterError> {
    let references = references(item, slot)?;
    if references.len() != 1
        || references[0].item_type.as_str() != item_type
        || references[0].quantity != 1
    {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    Ok(&references[0])
}

fn require_localizations(
    item: &BehaviorItemContext,
    fields: &[&str],
) -> Result<(), BehaviorAdapterError> {
    for locale in ["eng", "zhs"] {
        let localization = item
            .localizations
            .iter()
            .find(|(locale_id, _)| locale_id.as_str() == locale)
            .map(|(_, value)| value)
            .ok_or(BehaviorAdapterError::InvalidContext)?;
        for field in fields {
            localization_value(localization, field)?;
        }
    }
    Ok(())
}

fn localization_value<'a>(
    localization: &'a BTreeMap<LocalizationFieldId, String>,
    field: &str,
) -> Result<&'a str, BehaviorAdapterError> {
    localization
        .iter()
        .find(|(field_id, _)| field_id.as_str() == field)
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .ok_or(BehaviorAdapterError::InvalidContext)
}

fn require_paths(item: &BehaviorItemContext, roles: &[&str]) -> Result<(), BehaviorAdapterError> {
    for role in roles {
        path_for_role(item, role)?;
    }
    Ok(())
}

fn path_for_role<'a>(
    item: &'a BehaviorItemContext,
    role: &str,
) -> Result<&'a str, BehaviorAdapterError> {
    let mut matches = item
        .resources
        .values()
        .filter_map(|resource| resource.published_paths.get(role));
    let path = matches.next().ok_or(BehaviorAdapterError::InvalidContext)?;
    if matches.next().is_some() {
        return Err(BehaviorAdapterError::InvalidContext);
    }
    Ok(path)
}

fn namespace_for(mod_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(mod_id.as_bytes()));
    format!("Ats{}", &digest[..16])
}

fn class_name(prefix: &str, definition_hash: &Sha256Digest) -> String {
    format!("Ats{prefix}{}", &definition_hash.as_str()[..16])
}

fn model_key(context: &BehaviorRenderContext) -> String {
    let namespace = namespace_for(&context.mod_id).to_uppercase();
    let class = class_name(
        &pascal(context.item.item_type.as_str()),
        &context.item.definition_hash,
    );
    format!("{namespace}-{}", upper_snake(&class))
}

fn pascal(value: &str) -> String {
    let mut result = String::new();
    let mut uppercase = true;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            uppercase = true;
        } else if uppercase {
            result.push(character.to_ascii_uppercase());
            uppercase = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn upper_snake(value: &str) -> String {
    let mut result = String::new();
    let mut previous_was_lowercase = false;
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 && previous_was_lowercase {
            result.push('_');
        }
        result.push(character.to_ascii_uppercase());
        previous_was_lowercase = character.is_ascii_lowercase();
    }
    result
}

fn target_type(value: &str) -> Result<&'static str, BehaviorAdapterError> {
    match value {
        "none" => Ok("None"),
        "self" => Ok("Self"),
        "any_enemy" => Ok("AnyEnemy"),
        "all_enemies" => Ok("AllEnemies"),
        "random_enemy" => Ok("RandomEnemy"),
        "any_player" => Ok("AnyPlayer"),
        "any_ally" => Ok("AnyAlly"),
        "all_allies" => Ok("AllAllies"),
        "targeted_no_creature" => Ok("TargetedNoCreature"),
        "osty" => Ok("Osty"),
        _ => Err(BehaviorAdapterError::InvalidContext),
    }
}

fn localization_suffix(table: &str, value: &str) -> String {
    match (table, value) {
        ("characters", "end_turn_ping_alive") => return "banter.alive.endTurnPing".into(),
        ("characters", "end_turn_ping_dead") => return "banter.dead.endTurnPing".into(),
        (_, "name") => return "title".into(),
        _ => {}
    }
    let mut result = String::new();
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            result.push(character.to_ascii_uppercase());
            uppercase = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn csharp_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing one string cannot fail")
}

fn valid_hex_color(value: &str) -> bool {
    let value = value.strip_prefix('#').unwrap_or(value);
    matches!(value.len(), 6 | 8) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

const CHARACTER_LOCALIZATION_FIELDS: &[&str] = &[
    "title",
    "title_object",
    "description",
    "pronoun_object",
    "pronoun_subject",
    "pronoun_possessive",
    "possessive_adjective",
    "aroma_principle",
    "end_turn_ping_alive",
    "end_turn_ping_dead",
    "event_death_prevention",
    "gold_monologue",
    "cards_modifier_title",
    "cards_modifier_description",
];

#[cfg(test)]
mod tests {
    use super::*;
    use ats_game_context::{
        BehaviorCapabilitySet, BehaviorItemReference, BehaviorResourceBinding, CapabilityCatalog,
        CapabilityParameterSpec, CapabilityParameterType, CapabilitySpec, GamePackLoader,
    };
    use ats_kernel::{
        BehaviorCapabilityId, CapabilityCatalogId, CapabilityParameterId, GamePackId, ItemFieldId,
        ItemId, ItemReferenceSlotId, ItemTypeId, LocaleId, ResourceId,
    };

    fn digest(byte: char) -> Sha256Digest {
        Sha256Digest::parse(byte.to_string().repeat(64)).unwrap()
    }

    fn capability(id: &str) -> CapabilitySpec {
        CapabilitySpec {
            id: BehaviorCapabilityId::parse(id).unwrap(),
            description: "Fixture capability.".into(),
            max_invocations_per_item: 1,
            parameters: vec![CapabilityParameterSpec {
                id: CapabilityParameterId::parse("amount").unwrap(),
                description: "Effect amount.".into(),
                required: true,
                value: CapabilityParameterType::Integer { min: 1, max: 99 },
            }],
        }
    }

    fn catalog(item_type: &str, capability_ids: &[&str]) -> CapabilityCatalog {
        let adapter = Sts2BehaviorAdapter::new().identity().clone();
        CapabilityCatalog {
            schema_version: 1,
            id: CapabilityCatalogId::parse("game.sts2.fixture").unwrap(),
            version: SchemaVersion::new(1).unwrap(),
            adapter,
            capabilities: capability_ids.iter().map(|id| capability(id)).collect(),
            item_types: vec![BehaviorCapabilitySet {
                item_type: ItemTypeId::parse(item_type).unwrap(),
                allowed_capabilities: capability_ids
                    .iter()
                    .map(|id| BehaviorCapabilityId::parse(*id).unwrap())
                    .collect(),
                min_invocations: u32::try_from(capability_ids.len()).unwrap(),
                max_invocations: u32::try_from(capability_ids.len()).unwrap(),
            }],
        }
    }

    fn proposal(
        item_type: &str,
        capability_id: Option<&str>,
    ) -> (CapabilityCatalog, BehaviorProposal) {
        proposal_with_capabilities(item_type, &capability_id.into_iter().collect::<Vec<_>>())
    }

    fn proposal_with_capabilities(
        item_type: &str,
        capability_ids: &[&str],
    ) -> (CapabilityCatalog, BehaviorProposal) {
        let catalog = catalog(item_type, capability_ids);
        let proposal = BehaviorProposal {
            schema_version: 1,
            item_id: ItemId::parse(format!("fixture-{item_type}")).unwrap(),
            item_type: ItemTypeId::parse(item_type).unwrap(),
            definition_hash: digest(match item_type {
                "character" => 'c',
                "card" => 'd',
                "relic" => 'e',
                "potion" => 'a',
                "power" => 'b',
                _ => 'f',
            }),
            catalog: catalog.identity().unwrap(),
            adapter: catalog.adapter.clone(),
            invocations: capability_ids
                .iter()
                .map(|id| CapabilityInvocation {
                    capability_id: BehaviorCapabilityId::parse(*id).unwrap(),
                    arguments: BTreeMap::from([(
                        CapabilityParameterId::parse("amount").unwrap(),
                        CapabilityValue::Integer(7),
                    )]),
                })
                .collect(),
        };
        (catalog, proposal)
    }

    fn context(proposal: &BehaviorProposal) -> BehaviorRenderContext {
        let mut item = BehaviorItemContext {
            item_id: proposal.item_id.clone(),
            item_type: proposal.item_type.clone(),
            definition_hash: proposal.definition_hash.clone(),
            canonical_fields: BTreeMap::new(),
            localizations: BTreeMap::new(),
            references: BTreeMap::new(),
            resources: BTreeMap::new(),
        };
        for locale in ["eng", "zhs"] {
            let fields = match proposal.item_type.as_str() {
                "character" => CHARACTER_LOCALIZATION_FIELDS,
                _ => &["name", "description"],
            };
            item.localizations.insert(
                LocaleId::parse(locale).unwrap(),
                fields
                    .iter()
                    .map(|field| {
                        (
                            LocalizationFieldId::parse(*field).unwrap(),
                            format!("{locale}-{field}"),
                        )
                    })
                    .collect(),
            );
        }
        BehaviorRenderContext {
            mod_id: "FixtureMod".into(),
            game_pack_id: GamePackId::parse("sts2").unwrap(),
            game_pack_sha256: digest('a'),
            truth_snapshot_id: digest('b'),
            catalog: proposal.catalog.clone(),
            adapter: proposal.adapter.clone(),
            item,
        }
    }

    fn add_field(context: &mut BehaviorRenderContext, id: &str, value: CapabilityValue) {
        context
            .item
            .canonical_fields
            .insert(ItemFieldId::parse(id).unwrap(), value);
    }

    fn add_reference(context: &mut BehaviorRenderContext, slot: &str, item_type: &str) {
        add_reference_with_hash(context, slot, item_type, digest('f'));
    }

    fn add_reference_with_hash(
        context: &mut BehaviorRenderContext,
        slot: &str,
        item_type: &str,
        definition_hash: Sha256Digest,
    ) {
        context.item.references.insert(
            ItemReferenceSlotId::parse(slot).unwrap(),
            vec![BehaviorItemReference {
                item_id: ItemId::parse(format!("owner-{item_type}")).unwrap(),
                item_type: ItemTypeId::parse(item_type).unwrap(),
                definition_hash,
                quantity: 1,
            }],
        );
    }

    fn add_paths(context: &mut BehaviorRenderContext, roles: &[&str]) {
        for (index, role) in roles.iter().enumerate() {
            let resource_id = ResourceId::parse(format!("fixture.resource.r{index:03}")).unwrap();
            context.item.resources.insert(
                resource_id.clone(),
                BehaviorResourceBinding {
                    resource_id,
                    selected_version: digest(char::from_digit(index as u32, 16).unwrap_or('e')),
                    published_paths: BTreeMap::from([(
                        (*role).into(),
                        format!("FixtureMod/images/{index}.png"),
                    )]),
                },
            );
        }
    }

    #[test]
    fn built_in_pack_selects_this_exact_adapter() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        assert_eq!(
            pack.behavior_adapter(),
            Sts2BehaviorAdapter::new().identity()
        );
    }

    #[test]
    fn adapter_identity_hash_matches_normalized_production_source() {
        let source = include_str!("behavior.rs").replace("\r\n", "\n");
        let production = source
            .split_once("#[cfg(test)]")
            .expect("behavior implementation keeps tests after production code")
            .0;
        let normalized =
            production.replacen(BEHAVIOR_ADAPTER_IMPLEMENTATION_SHA256, &"0".repeat(64), 1);
        assert_eq!(
            format!("{:x}", Sha256::digest(normalized.as_bytes())),
            BEHAVIOR_ADAPTER_IMPLEMENTATION_SHA256
        );
    }

    #[test]
    fn card_render_is_byte_deterministic_and_contains_no_model_authored_native_names() {
        let (catalog, proposal) = proposal("card", Some(CARD_DEAL_DAMAGE));
        let mut context = context(&proposal);
        for (id, value) in [
            ("pool", CapabilityValue::Choice("custom_character".into())),
            ("card_type", CapabilityValue::Choice("attack".into())),
            ("rarity", CapabilityValue::Choice("common".into())),
            ("target", CapabilityValue::Choice("any_enemy".into())),
            ("base_cost", CapabilityValue::Integer(1)),
        ] {
            add_field(&mut context, id, value);
        }
        add_reference(&mut context, "owner_character", "character");
        add_paths(&mut context, &["card.portrait", "card.big"]);
        let mut registry = ats_game_context::BehaviorAdapterRegistry::new();
        registry.register(Sts2BehaviorAdapter::new()).unwrap();
        let first = registry
            .render(&catalog.adapter, &catalog, &context, &proposal)
            .unwrap();
        let second = registry
            .render(&catalog.adapter, &catalog, &context, &proposal)
            .unwrap();
        assert_eq!(first, second);
        let source = String::from_utf8(first.files[0].bytes.clone()).unwrap();
        assert!(source.contains("DamageCmd.Attack"));
        assert!(!source.contains(proposal.item_id.as_str()));
    }

    #[test]
    fn unsupported_declared_timing_is_a_local_adapter_failure() {
        let (catalog, mut proposal) = proposal("card", Some(CARD_DEAL_DAMAGE));
        proposal.invocations[0].capability_id =
            BehaviorCapabilityId::parse("card.turn_end.deal_damage").unwrap();
        let context = context(&proposal);
        assert_eq!(
            Sts2BehaviorAdapter::new()
                .validate_ir(&context, &proposal)
                .unwrap_err(),
            BehaviorAdapterError::UnsupportedCapability
        );
        assert!(proposal.validate(&catalog).is_err());
    }

    #[test]
    fn all_five_structured_item_types_have_deterministic_renderers() {
        for (item_type, capability_id) in [
            ("character", None),
            ("card", Some(CARD_GAIN_BLOCK)),
            ("relic", Some(RELIC_COMBAT_START_GAIN_BLOCK)),
            ("potion", Some(POTION_ON_USE_GAIN_BLOCK)),
            ("power", Some(POWER_TURN_START_GAIN_BLOCK)),
        ] {
            let (_catalog, proposal) = proposal(item_type, capability_id);
            let mut context = context(&proposal);
            match item_type {
                "character" => {
                    for (id, value) in [
                        (
                            "visual_profile",
                            CapabilityValue::Choice("placeholder".into()),
                        ),
                        ("placeholder_id", CapabilityValue::Choice("ironclad".into())),
                        ("name_color", CapabilityValue::Text("#FF6B35".into())),
                        ("gender", CapabilityValue::Choice("neutral".into())),
                        ("starting_hp", CapabilityValue::Integer(75)),
                        ("starting_gold", CapabilityValue::Integer(99)),
                        ("max_energy", CapabilityValue::Integer(3)),
                    ] {
                        add_field(&mut context, id, value);
                    }
                    for (slot, target) in [
                        ("starting_deck", "card"),
                        ("cards", "card"),
                        ("starting_relics", "relic"),
                        ("relics", "relic"),
                        ("potions", "potion"),
                        ("powers", "power"),
                    ] {
                        context
                            .item
                            .references
                            .insert(ItemReferenceSlotId::parse(slot).unwrap(), Vec::new());
                        assert!(target.len() > 3);
                    }
                }
                "card" => {
                    for (id, value) in [
                        ("pool", CapabilityValue::Choice("custom_character".into())),
                        ("card_type", CapabilityValue::Choice("skill".into())),
                        ("rarity", CapabilityValue::Choice("common".into())),
                        ("target", CapabilityValue::Choice("self".into())),
                        ("base_cost", CapabilityValue::Integer(1)),
                    ] {
                        add_field(&mut context, id, value);
                    }
                    add_reference(&mut context, "owner_character", "character");
                    add_paths(&mut context, &["card.portrait", "card.big"]);
                }
                "relic" => {
                    add_field(
                        &mut context,
                        "rarity",
                        CapabilityValue::Choice("starter".into()),
                    );
                    add_reference(&mut context, "owner_character", "character");
                    add_paths(
                        &mut context,
                        &["relic.normal", "relic.outline", "relic.big"],
                    );
                }
                "potion" => {
                    add_field(
                        &mut context,
                        "rarity",
                        CapabilityValue::Choice("common".into()),
                    );
                    add_field(
                        &mut context,
                        "usage",
                        CapabilityValue::Choice("combat_only".into()),
                    );
                    add_field(
                        &mut context,
                        "target",
                        CapabilityValue::Choice("self".into()),
                    );
                    add_reference(&mut context, "owner_character", "character");
                    add_paths(&mut context, &["potion.icon"]);
                }
                "power" => {
                    add_field(
                        &mut context,
                        "power_type",
                        CapabilityValue::Choice("buff".into()),
                    );
                    add_field(
                        &mut context,
                        "stack_type",
                        CapabilityValue::Choice("counter".into()),
                    );
                    add_field(
                        &mut context,
                        "instance_type",
                        CapabilityValue::Choice("none".into()),
                    );
                    add_field(
                        &mut context,
                        "allow_negative",
                        CapabilityValue::Boolean(false),
                    );
                    add_paths(&mut context, &["power.icon", "power.big"]);
                }
                _ => unreachable!(),
            }
            let adapter = Sts2BehaviorAdapter::new();
            adapter.validate_ir(&context, &proposal).unwrap();
            let first = adapter.render(&context, &proposal).unwrap();
            let second = adapter.render(&context, &proposal).unwrap();
            assert_eq!(first, second);
        }
    }

    #[test]
    #[ignore = "requires ATS_STS2_ASSEMBLY_PATH and the real STS2/BaseLib toolchain"]
    fn rendered_five_item_closure_compiles_against_real_sts2_baselib() {
        use std::fs;
        use std::path::PathBuf;
        use std::process::Command;

        let assembly = std::env::var_os("ATS_STS2_ASSEMBLY_PATH")
            .map(PathBuf::from)
            .expect("ATS_STS2_ASSEMBLY_PATH must point to sts2.dll");
        assert_eq!(
            assembly.file_name().and_then(|name| name.to_str()),
            Some("sts2.dll")
        );
        assert!(assembly.is_file(), "STS2 assembly must exist");
        let data_dir = assembly.parent().expect("sts2.dll has a parent directory");
        let harmony = data_dir.join("0Harmony.dll");
        assert!(harmony.is_file(), "0Harmony.dll must exist beside sts2.dll");

        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("AdapterFixture.csproj");
        let project_xml = format!(
            r#"<Project Sdk="Godot.NET.Sdk/4.5.1">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
  <ItemGroup>
    <Reference Include="0Harmony"><HintPath>{}</HintPath><Private>false</Private></Reference>
    <Reference Include="sts2"><HintPath>{}</HintPath><Private>false</Private></Reference>
    <PackageReference Include="Alchyr.Sts2.BaseLib" Version="3.3.8" PrivateAssets="All" />
  </ItemGroup>
</Project>
"#,
            xml_path(&harmony),
            xml_path(&assembly),
        );
        fs::write(&project, project_xml).unwrap();

        let fixture_specs: [(&str, &[&str]); 5] = [
            ("character", &[]),
            (
                "card",
                &[
                    CARD_DEAL_DAMAGE,
                    CARD_GAIN_BLOCK,
                    CARD_DRAW_CARDS,
                    CARD_GAIN_ENERGY,
                ],
            ),
            ("relic", &[RELIC_COMBAT_START_GAIN_BLOCK]),
            ("potion", &[POTION_ON_USE_GAIN_BLOCK]),
            ("power", &[POWER_TURN_START_GAIN_BLOCK]),
        ];
        for (item_type, capabilities) in fixture_specs {
            let (_catalog, proposal) = proposal_with_capabilities(item_type, capabilities);
            let mut context = context(&proposal);
            configure_compile_context(&mut context, item_type);
            let bundle = Sts2BehaviorAdapter::new()
                .render(&context, &proposal)
                .unwrap();
            for file in bundle.files.iter().filter(|file| file.role == "source") {
                let path = temp.path().join(&file.relative_path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, &file.bytes).unwrap();
            }
        }

        let output = Command::new("dotnet")
            .arg("build")
            .arg(&project)
            .arg("--nologo")
            .arg("--configuration")
            .arg("Release")
            .output()
            .expect("dotnet must be available");
        assert!(
            output.status.success(),
            "real STS2/BaseLib fixture compilation failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn configure_compile_context(context: &mut BehaviorRenderContext, item_type: &str) {
        match item_type {
            "character" => {
                for (id, value) in [
                    (
                        "visual_profile",
                        CapabilityValue::Choice("placeholder".into()),
                    ),
                    ("placeholder_id", CapabilityValue::Choice("ironclad".into())),
                    ("name_color", CapabilityValue::Text("#FF6B35".into())),
                    ("gender", CapabilityValue::Choice("neutral".into())),
                    ("starting_hp", CapabilityValue::Integer(75)),
                    ("starting_gold", CapabilityValue::Integer(99)),
                    ("max_energy", CapabilityValue::Integer(3)),
                ] {
                    add_field(context, id, value);
                }
                for (slot, target, hash) in [
                    ("starting_deck", "card", digest('d')),
                    ("cards", "card", digest('d')),
                    ("starting_relics", "relic", digest('e')),
                    ("relics", "relic", digest('e')),
                    ("potions", "potion", digest('a')),
                    ("powers", "power", digest('b')),
                ] {
                    add_reference_with_hash(context, slot, target, hash);
                }
            }
            "card" => {
                for (id, value) in [
                    ("pool", CapabilityValue::Choice("custom_character".into())),
                    ("card_type", CapabilityValue::Choice("attack".into())),
                    ("rarity", CapabilityValue::Choice("common".into())),
                    ("target", CapabilityValue::Choice("any_enemy".into())),
                    ("base_cost", CapabilityValue::Integer(1)),
                ] {
                    add_field(context, id, value);
                }
                add_reference_with_hash(context, "owner_character", "character", digest('c'));
                add_paths(context, &["card.portrait", "card.big"]);
            }
            "relic" => {
                add_field(context, "rarity", CapabilityValue::Choice("starter".into()));
                add_reference_with_hash(context, "owner_character", "character", digest('c'));
                add_paths(context, &["relic.normal", "relic.outline", "relic.big"]);
            }
            "potion" => {
                add_field(context, "rarity", CapabilityValue::Choice("common".into()));
                add_field(
                    context,
                    "usage",
                    CapabilityValue::Choice("combat_only".into()),
                );
                add_field(context, "target", CapabilityValue::Choice("self".into()));
                add_reference_with_hash(context, "owner_character", "character", digest('c'));
                add_paths(context, &["potion.icon"]);
            }
            "power" => {
                add_field(
                    context,
                    "power_type",
                    CapabilityValue::Choice("buff".into()),
                );
                add_field(
                    context,
                    "stack_type",
                    CapabilityValue::Choice("counter".into()),
                );
                add_field(
                    context,
                    "instance_type",
                    CapabilityValue::Choice("none".into()),
                );
                add_field(context, "allow_negative", CapabilityValue::Boolean(false));
                add_paths(context, &["power.icon", "power.big"]);
            }
            _ => unreachable!(),
        }
    }

    fn xml_path(path: &std::path::Path) -> String {
        path.to_string_lossy()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
}
