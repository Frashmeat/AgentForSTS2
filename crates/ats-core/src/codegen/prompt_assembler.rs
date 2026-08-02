//! Codegen prompt 装配——把请求 + 知识包渲染成完整 LLM prompt。
//!
//! 与 Python 端的简化：移除了 "legacy fallback"（当 resolver 缺失/返回空时回退到
//! Pack guidance）。Rust 端 resolver 从固定 Game Pack 读取已校验 guidance，
//! 所以不需要磁盘 fallback。Facts 当前为 stub（stage 2.2 之前为空），
//! 我们在 prompt 中明确告知 LLM "结构化代码事实暂不可用，请直接读源码"。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codegen::models::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
    asset_localization_key_segment,
};
use crate::game_pack::{AssetResourceSpec, VerifiedGameContext};
use crate::knowledge::{
    KnowledgePacket, KnowledgeQuery, KnowledgeScenario, SnapshotCodeFactsError,
    Sts2KnowledgeResolver,
};
use crate::prompting::{PromptContextAssembler, PromptError, PromptLoader};

const FACTS_STUB_MESSAGE: &str = "Structured code facts are not available for this request. \
Use only the inlined Rules And Guidance and project context below. Do not invent API signatures or claim to read local files.";

const FACTS_STUB_WARNING: &str = "### Warnings\n- Structured code facts are unavailable in this build. \
Treat the guidance summary as best-effort context, not as the authoritative code fact source.";

pub struct PromptAssembler {
    pub loader: PromptLoader,
    pub resolver: Sts2KnowledgeResolver,
    pub context_assembler: PromptContextAssembler,
}

#[derive(Debug, Clone)]
pub struct AssetPromptAssembly {
    pub prompt: String,
    pub evidence: Vec<GenerationEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationEvidence {
    pub source: String,
    pub symbol: String,
    pub purpose: String,
    pub bounded_excerpt: String,
}

#[derive(Debug, Error)]
pub enum PromptAssemblyError {
    #[error(transparent)]
    Template(#[from] PromptError),
    #[error(transparent)]
    Knowledge(#[from] SnapshotCodeFactsError),
    #[error("serialize prompt contract: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("game pack `{game_pack_id}` does not declare structured asset type `{asset_type}`")]
    UnsupportedAssetType {
        game_pack_id: String,
        asset_type: String,
    },
}

impl PromptAssembler {
    #[must_use]
    pub fn new(
        loader: PromptLoader,
        resolver: Sts2KnowledgeResolver,
        context_assembler: PromptContextAssembler,
    ) -> Self {
        Self {
            loader,
            resolver,
            context_assembler,
        }
    }

    /// 默认装配：用内嵌 prompts + 默认 resolver。生产代码可继续 `.with_*` 替换。
    #[must_use]
    pub fn built_in() -> Self {
        Self::new(
            PromptLoader::built_in(),
            Sts2KnowledgeResolver::default(),
            PromptContextAssembler,
        )
    }

    pub fn assemble_asset_prompt(
        &self,
        request: &AssetCodegenRequest,
        context: &VerifiedGameContext,
    ) -> Result<String, PromptAssemblyError> {
        self.assemble_asset_prompt_with_evidence(request, context)
            .map(|assembly| assembly.prompt)
    }

    pub fn assemble_asset_prompt_with_evidence(
        &self,
        request: &AssetCodegenRequest,
        context: &VerifiedGameContext,
    ) -> Result<AssetPromptAssembly, PromptAssemblyError> {
        let resource_spec = context
            .pack()
            .resource_spec(&request.asset_type)
            .ok_or_else(|| PromptAssemblyError::UnsupportedAssetType {
                game_pack_id: context.game_pack_id().into(),
                asset_type: request.asset_type.clone(),
            })?;
        let canonical_asset_type = resource_spec.id.as_str();
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            domain: context.game_pack_id().into(),
            asset_type: Some(canonical_asset_type.to_string()),
            project_root: Some(request.project_root.clone()),
            requirements: Some(request.design_description.clone()),
            item_name: Some(request.asset_name.clone()),
            symbols: Vec::new(),
            group_asset_types: Vec::new(),
        };
        let knowledge = self.resolve_knowledge(&query, context)?;

        let img_list = format_image_list(&request.image_paths);
        let zhs_hint = if request.name_zhs.is_empty() {
            String::new()
        } else {
            format!(
                "\nSimplified Chinese display name (name_zhs): {}",
                request.name_zhs
            )
        };
        let project_root = path_to_posix(&request.project_root);
        let (mod_name, project_context) = project_context(&request.project_root);
        let localization_table = resource_spec.localization.table.as_str();
        let localization_key = format!(
            "{}-{}",
            mod_name.to_ascii_uppercase(),
            asset_localization_key_segment(&request.asset_name)
        );
        let asset_lookup = "No file or tool lookup is available during this request. Use the inlined Code Facts, Rules And Guidance, and project context only.";
        let localization_contract = render_localization_contract(resource_spec);
        let localization_example = render_localization_example(resource_spec, &localization_key)?;

        let vars = HashMap::from([
            ("asset_name", request.asset_name.as_str()),
            ("asset_type", canonical_asset_type),
            ("design_description", request.design_description.as_str()),
            ("facts", knowledge.facts.as_str()),
            ("guidance", knowledge.guidance.as_str()),
            ("lookup", asset_lookup),
            ("knowledge_warnings", knowledge.warnings.as_str()),
            ("img_list", img_list.as_str()),
            ("localization_key", localization_key.as_str()),
            ("localization_table", localization_table),
            ("localization_contract", localization_contract.as_str()),
            ("localization_example", localization_example.as_str()),
            ("mod_name", mod_name.as_str()),
            ("project_context", project_context.as_str()),
            ("project_root", project_root.as_str()),
            ("zhs_hint", zhs_hint.as_str()),
        ]);
        let prompt = self.loader.render("codegen.asset_prompt", &vars)?;
        Ok(AssetPromptAssembly {
            prompt,
            evidence: knowledge.evidence,
        })
    }

    pub fn assemble_custom_code_prompt(
        &self,
        request: &CustomCodegenRequest,
        context: &VerifiedGameContext,
    ) -> Result<String, PromptAssemblyError> {
        self.assemble_custom_code_prompt_with_evidence(request, context)
            .map(|assembly| assembly.prompt)
    }

    pub fn assemble_custom_code_prompt_with_evidence(
        &self,
        request: &CustomCodegenRequest,
        context: &VerifiedGameContext,
    ) -> Result<AssetPromptAssembly, PromptAssemblyError> {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::CustomCodeCodegen),
            domain: context.game_pack_id().into(),
            asset_type: Some("custom_code".into()),
            project_root: Some(request.project_root.clone()),
            requirements: Some(request.description.clone()),
            item_name: Some(request.name.clone()),
            symbols: Vec::new(),
            group_asset_types: Vec::new(),
        };
        let knowledge = self.resolve_knowledge(&query, context)?;

        let build_note = "NOTE: Godot headless export always exits with code -1, but if MSBuild reports '0 Error(s)' and the overall dotnet exit code is 0 — that is SUCCESS. Do NOT re-run just because of Godot's -1.";
        let build_steps = if request.skip_build {
            "5. Do NOT run dotnet publish — the build will be done later after all assets are created.".to_string()
        } else {
            format!(
                "5. Run `dotnet publish` (NOT dotnet build). {build_note}\n   Fix any actual compilation errors and re-run until it succeeds.\n6. Confirm the build succeeded and files deployed to mods folder."
            )
        };

        let api_ref = snapshot_lookup_root(context, "game");
        let project_root = path_to_posix(&request.project_root);
        let mod_name = path_basename(&request.project_root);

        let vars = HashMap::from([
            ("api_ref_path", api_ref.as_str()),
            ("build_steps", build_steps.as_str()),
            ("description", request.description.as_str()),
            ("facts", knowledge.facts.as_str()),
            ("guidance", knowledge.guidance.as_str()),
            ("lookup", knowledge.lookup.as_str()),
            ("knowledge_warnings", knowledge.warnings.as_str()),
            (
                "implementation_notes",
                request.implementation_notes.as_str(),
            ),
            ("mod_name", mod_name.as_str()),
            ("name", request.name.as_str()),
            ("project_root", project_root.as_str()),
        ]);
        Ok(AssetPromptAssembly {
            prompt: self.loader.render("codegen.custom_code_prompt", &vars)?,
            evidence: knowledge.evidence,
        })
    }

    pub fn assemble_asset_group_prompt(
        &self,
        request: &AssetGroupRequest,
        context: &VerifiedGameContext,
    ) -> Result<String, PromptAssemblyError> {
        let symbols: Vec<String> = request.assets.iter().map(|a| a.item.name.clone()).collect();
        let group_asset_types: Vec<String> = request
            .assets
            .iter()
            .map(|a| asset_type_str(&a.item.item_type).into())
            .collect();
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetGroupCodegen),
            domain: context.game_pack_id().into(),
            asset_type: None,
            project_root: Some(request.project_root.clone()),
            requirements: None,
            item_name: None,
            symbols,
            group_asset_types,
        };
        let knowledge = self.resolve_knowledge(&query, context)?;

        let assets_section = render_assets_section(&request.assets);
        let class_names = request
            .assets
            .iter()
            .map(|a| a.item.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let asset_count = request.assets.len().to_string();
        let project_root = path_to_posix(&request.project_root);
        let mod_name = path_basename(&request.project_root);
        let group_resource_contracts = render_group_resource_contracts(&request.assets, context)?;

        let vars = HashMap::from([
            ("asset_count", asset_count.as_str()),
            ("assets_section", assets_section.as_str()),
            ("class_names", class_names.as_str()),
            ("facts", knowledge.facts.as_str()),
            ("guidance", knowledge.guidance.as_str()),
            (
                "group_resource_contracts",
                group_resource_contracts.as_str(),
            ),
            ("lookup", knowledge.lookup.as_str()),
            ("knowledge_warnings", knowledge.warnings.as_str()),
            ("mod_name", mod_name.as_str()),
            ("project_root", project_root.as_str()),
        ]);
        Ok(self.loader.render("codegen.asset_group_prompt", &vars)?)
    }

    pub fn assemble_build_prompt(&self, max_attempts: u32) -> Result<String, PromptError> {
        let s = max_attempts.to_string();
        let vars = HashMap::from([("max_attempts", s.as_str())]);
        self.loader.render("codegen.build_prompt", &vars)
    }

    pub fn assemble_create_mod_project_prompt(
        &self,
        request: &ModProjectRequest,
    ) -> Result<String, PromptError> {
        let project_path = request.target_dir.join(&request.project_name);
        let project_path_str = path_to_posix(&project_path);
        let vars = HashMap::from([
            ("project_name", request.project_name.as_str()),
            ("project_path", project_path_str.as_str()),
        ]);
        self.loader
            .render("codegen.create_mod_project_prompt", &vars)
    }

    pub fn assemble_package_prompt(&self) -> Result<String, PromptError> {
        self.loader
            .render("codegen.package_prompt", &HashMap::new())
    }

    fn resolve_knowledge(
        &self,
        query: &KnowledgeQuery,
        context: &VerifiedGameContext,
    ) -> Result<ResolvedKnowledge, PromptAssemblyError> {
        let packet = self.resolver.resolve(query, context)?;
        Ok(ResolvedKnowledge::from_packet(
            &packet,
            &self.context_assembler,
        ))
    }
}

struct ResolvedKnowledge {
    facts: String,
    guidance: String,
    lookup: String,
    warnings: String,
    evidence: Vec<GenerationEvidence>,
}

impl ResolvedKnowledge {
    fn from_packet(packet: &KnowledgePacket, assembler: &PromptContextAssembler) -> Self {
        let evidence = packet
            .facts
            .iter()
            .flat_map(|fact| {
                fact.evidence_paths.iter().map(|source| GenerationEvidence {
                    source: source.clone(),
                    symbol: fact.key.clone(),
                    purpose: fact.title.clone(),
                    bounded_excerpt: fact.body.clone(),
                })
            })
            .collect();
        let mut ctx = assembler.assemble(packet);
        let facts = ctx.remove("facts").unwrap_or_default();
        let facts = if facts.trim().is_empty() {
            FACTS_STUB_MESSAGE.to_string()
        } else {
            facts
        };
        let raw_warnings = ctx.remove("knowledge_warnings").unwrap_or_default();
        let warnings = if raw_warnings.trim().is_empty() {
            String::new()
        } else if facts.trim().is_empty() || facts == FACTS_STUB_MESSAGE {
            // facts 为空 → 追加 stub warning
            format!("{raw_warnings}\n- {FACTS_STUB_WARNING}")
        } else {
            // facts 非空 → 真实知识源就位，不加 stub warning
            raw_warnings
        };
        Self {
            facts,
            guidance: ctx.remove("guidance").unwrap_or_default(),
            lookup: ctx.remove("lookup").unwrap_or_default(),
            warnings,
            evidence,
        }
    }
}

fn render_localization_contract(spec: &AssetResourceSpec) -> String {
    let rich_text_contract = if spec.localization.allowed_rich_text_tags.is_empty() {
        "Square-bracket rich-text syntax is not allowed.".to_string()
    } else {
        format!(
            "Allowed rich-text tags: {}. Use only exact, balanced open/close pairs drawn from this list; unknown tags and raw square-bracket text are invalid.",
            spec.localization.allowed_rich_text_tags.join(", ")
        )
    };
    format!(
        "Required locale maps: {}. Required key suffixes in every locale: {}. {}",
        spec.localization.locales.join(", "),
        spec.localization
            .required_suffixes
            .iter()
            .map(|suffix| format!(".{suffix}"))
            .collect::<Vec<_>>()
            .join(", "),
        rich_text_contract,
    )
}

fn render_localization_example(
    spec: &AssetResourceSpec,
    localization_key: &str,
) -> Result<String, serde_json::Error> {
    let mut localization = serde_json::Map::new();
    for locale in &spec.localization.locales {
        let entries = spec
            .localization
            .required_suffixes
            .iter()
            .map(|suffix| {
                (
                    format!("{localization_key}.{suffix}"),
                    serde_json::Value::String(format!("{locale} {suffix}")),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        localization.insert(locale.clone(), serde_json::Value::Object(entries));
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "csharp": "using ...;\n\nnamespace ...;\n\npublic sealed class ... {}",
        "localization": localization,
    }))
}

fn render_group_resource_contracts(
    assets: &[crate::codegen::AssetGroupItem],
    context: &VerifiedGameContext,
) -> Result<String, PromptAssemblyError> {
    let mut rendered = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for asset in assets {
        let asset_type = asset_type_str(&asset.item.item_type);
        if asset_type == "custom_code" || !seen.insert(asset_type) {
            continue;
        }
        let spec = context.pack().resource_spec(asset_type).ok_or_else(|| {
            PromptAssemblyError::UnsupportedAssetType {
                game_pack_id: context.game_pack_id().into(),
                asset_type: asset_type.into(),
            }
        })?;
        rendered.push(format!(
            "- `{}`: locales {}; table `{}`; required suffixes {}; allowed rich-text tags {}",
            spec.id,
            spec.localization.locales.join(", "),
            spec.localization.table,
            spec.localization
                .required_suffixes
                .iter()
                .map(|suffix| format!(".{suffix}"))
                .collect::<Vec<_>>()
                .join(", "),
            if spec.localization.allowed_rich_text_tags.is_empty() {
                "none".into()
            } else {
                spec.localization.allowed_rich_text_tags.join(", ")
            }
        ));
    }
    if rendered.is_empty() {
        Ok("- No structured asset localization is required.".into())
    } else {
        Ok(rendered.join("\n"))
    }
}

fn asset_type_str(asset_type: &crate::planning::AssetItemType) -> &str {
    asset_type.as_str()
}

fn format_image_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| format!("  - {}", path_to_posix(p)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_assets_section(assets: &[crate::codegen::AssetGroupItem]) -> String {
    let mut buf = String::new();
    for (idx, asset) in assets.iter().enumerate() {
        let item = &asset.item;
        let img_list = if asset.image_paths.is_empty() {
            "      (no image — code-only asset)".to_string()
        } else {
            asset
                .image_paths
                .iter()
                .map(|p| format!("      - {}", path_to_posix(p)))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let zhs = if item.name_zhs.is_empty() {
            String::new()
        } else {
            format!("\n  - Chinese name: {}", item.name_zhs)
        };
        let depends = if item.depends_on_item_ids.is_empty() {
            "none".to_string()
        } else {
            item.depends_on_item_ids.join(", ")
        };
        buf.push_str(&format!(
            "\n### Asset {idx}: [{type_}] {name}{zhs}\n  - Description: {desc}\n  - Implementation notes: {notes}\n  - Image files:\n{img_list}\n  - Depends on: {depends}\n",
            idx = idx + 1,
            type_ = asset_type_str(&item.item_type),
            name = item.name,
            desc = item.description,
            notes = item.implementation_notes,
        ));
    }
    buf.trim().to_string()
}

fn snapshot_lookup_root(context: &VerifiedGameContext, source_id: &str) -> String {
    format!("snapshot://{}/{source_id}/", context.snapshot_id())
}

fn path_to_posix(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn path_basename(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn project_context(project_root: &Path) -> (String, String) {
    let meta = std::fs::read_to_string(project_root.join("project.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<crate::project::ProjectMeta>(&text).ok());
    let mod_name = meta
        .as_ref()
        .map(|m| m.csharp_name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path_basename(project_root));
    let main_file = std::fs::read_to_string(project_root.join("MainFile.cs"))
        .unwrap_or_else(|_| "(MainFile.cs is unavailable; generation will fail validation)".into());
    let context =
        format!("Resolved ModId: {mod_name}\n\nCurrent MainFile.cs:\n```csharp\n{main_file}\n```");
    (mod_name, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::test_support::fixture_game_context;

    fn context(temp: &tempfile::TempDir) -> VerifiedGameContext {
        fixture_game_context(temp.path(), &[], &[])
    }

    #[test]
    fn asset_prompt_contains_required_fragments() {
        let assembler = PromptAssembler::built_in();
        let temp = tempfile::TempDir::new().unwrap();
        let context = context(&temp);
        let request = AssetCodegenRequest {
            asset_type: "card".into(),
            asset_name: "DemoCard".into(),
            design_description: "测试卡".into(),
            project_root: PathBuf::from("E:/mods/demo_mod"),
            image_paths: vec![PathBuf::from("E:/mods/demo_mod/img/demo.png")],
            name_zhs: "演示卡".into(),
            ..Default::default()
        };
        let prompt = assembler
            .assemble_asset_prompt(&request, &context)
            .expect("assembly should succeed");
        assert!(prompt.contains("DemoCard"));
        assert!(prompt.contains("card"));
        assert!(prompt.contains("演示卡"));
        assert!(prompt.contains("demo.png"));
        assert!(prompt.contains("strict JSON"));
        assert!(prompt.contains("localization"));
        assert!(prompt.contains("Structured code facts are not available"));
        assert!(!prompt.contains("Read `MainFile.cs`"));
    }

    #[test]
    fn asset_prompt_does_not_delegate_file_or_build_operations() {
        let assembler = PromptAssembler::built_in();
        let temp = tempfile::TempDir::new().unwrap();
        let context = context(&temp);
        let request = AssetCodegenRequest {
            asset_type: "card".into(),
            asset_name: "X".into(),
            project_root: PathBuf::from("E:/mods/m"),
            skip_build: true,
            ..Default::default()
        };
        let prompt = assembler.assemble_asset_prompt(&request, &context).unwrap();
        assert!(!prompt.contains("Run `dotnet publish`"));
        assert!(prompt.contains("Do not read or write files"));
    }

    #[test]
    fn asset_prompt_normalizes_type_and_inlines_project_context() {
        let td = tempfile::TempDir::new().unwrap();
        let context = context(&td);
        std::fs::write(
            td.path().join("project.json"),
            r#"{"name":"demo","csharp_name":"DemoMod","game_id":"sts2","scaffolded":true,"generated_files":[],"build_output_dir":null}"#,
        )
        .unwrap();
        std::fs::write(
            td.path().join("MainFile.cs"),
            "namespace DemoMod; public class MainFile {}",
        )
        .unwrap();
        let request = AssetCodegenRequest {
            asset_type: "Relic".into(),
            asset_name: "EnergySeedRelic".into(),
            project_root: td.path().to_path_buf(),
            ..Default::default()
        };
        let prompt = PromptAssembler::built_in()
            .assemble_asset_prompt(&request, &context)
            .unwrap();
        assert!(prompt.contains("new relic"));
        assert!(prompt.contains("namespace DemoMod"));
        assert!(prompt.contains("DEMOMOD-ENERGY_SEED_RELIC"));
        assert!(prompt.contains("relics"));
        assert!(prompt.contains("Allowed rich-text tags: aqua, blue"));
        assert!(prompt.contains("[blue]1[/blue]"));
        assert!(prompt.contains("Never invent tags such as `[yellow]`"));
    }

    #[test]
    fn asset_prompt_inlines_current_behavior_evidence_and_records_manifest() {
        let td = tempfile::TempDir::new().unwrap();
        let context = fixture_game_context(
            td.path(),
            &[
                (
                    "MegaCrit.Sts2.Core.Models.Relics/Lantern.cs",
                    r#"namespace MegaCrit.Sts2.Core.Models.Relics;
public sealed class Lantern : RelicModel
{
    protected override IEnumerable<DynamicVar> CanonicalVars => new[] { new EnergyVar(1) };
    public override async Task AfterSideTurnStart(CombatSide side, CombatState combatState)
    {
        if (side == base.Owner.Creature.Side && combatState.RoundNumber <= 1)
            await PlayerCmd.GainEnergy(base.DynamicVars.Energy.BaseValue, base.Owner);
    }
}"#,
                ),
                (
                    "MegaCrit.Sts2.Core.Combat/CombatManager.cs",
                    r#"namespace MegaCrit.Sts2.Core.Combat;
public sealed class CombatManager
{
    public async Task StartSideTurn()
    {
        await SetupPlayerTurn(player);
        await Hook.AfterSideTurnStart(_state, _state.CurrentSide);
    }
    private async Task SetupPlayerTurn(Player player)
    {
        player.PlayerCombatState.ResetEnergy();
    }
}"#,
                ),
            ],
            &[],
        );
        std::fs::write(
            td.path().join("project.json"),
            r#"{"name":"demo","csharp_name":"DemoMod","game_id":"sts2","scaffolded":true,"generated_files":[],"build_output_dir":null}"#,
        )
        .unwrap();
        std::fs::write(
            td.path().join("MainFile.cs"),
            "namespace DemoMod; public class MainFile {}",
        )
        .unwrap();

        let request = AssetCodegenRequest {
            asset_type: "relic".into(),
            asset_name: "EnergySeedRelic".into(),
            design_description: "At the start of combat, gain 1 Energy. 战斗开始时获得1点能量。"
                .into(),
            project_root: td.path().to_path_buf(),
            ..Default::default()
        };
        let assembly = PromptAssembler::built_in()
            .assemble_asset_prompt_with_evidence(&request, &context)
            .unwrap();

        assert!(assembly.prompt.contains("Official similar implementation"));
        assert!(assembly.prompt.contains("RoundNumber <= 1"));
        assert!(assembly.prompt.contains("Hook.AfterSideTurnStart"));
        assert!(assembly.prompt.contains("ResetEnergy"));
        assert!(
            assembly
                .evidence
                .iter()
                .any(|item| item.source.contains("Lantern.cs")
                    && item.bounded_excerpt.contains("AfterSideTurnStart"))
        );
        assert!(
            assembly
                .evidence
                .iter()
                .any(|item| item.source.contains("CombatManager.cs")
                    && item.bounded_excerpt.contains("ResetEnergy"))
        );
        assert!(
            assembly
                .evidence
                .iter()
                .all(|item| item.source.starts_with("snapshot://"))
        );
    }

    #[test]
    fn custom_code_prompt_renders() {
        let assembler = PromptAssembler::built_in();
        let temp = tempfile::TempDir::new().unwrap();
        let context = context(&temp);
        let request = CustomCodegenRequest {
            name: "MyHook".into(),
            description: "钩子描述".into(),
            implementation_notes: "实现要点".into(),
            project_root: PathBuf::from("E:/mods/m"),
            skip_build: false,
        };
        let prompt = assembler
            .assemble_custom_code_prompt(&request, &context)
            .unwrap();
        assert!(prompt.contains("MyHook"));
        assert!(prompt.contains("钩子描述"));
        assert!(prompt.contains("实现要点"));
    }

    #[test]
    fn build_prompt_includes_max_attempts() {
        let assembler = PromptAssembler::built_in();
        let prompt = assembler.assemble_build_prompt(5).unwrap();
        assert!(prompt.contains("5"));
        assert!(prompt.contains("dotnet publish"));
    }

    #[test]
    fn create_mod_project_prompt_renders_path() {
        let assembler = PromptAssembler::built_in();
        let request = ModProjectRequest {
            project_name: "NewMod".into(),
            target_dir: PathBuf::from("E:/work"),
        };
        let prompt = assembler
            .assemble_create_mod_project_prompt(&request)
            .unwrap();
        assert!(prompt.contains("NewMod"));
        assert!(prompt.contains("E:/work/NewMod"));
    }

    #[test]
    fn package_prompt_non_empty() {
        let assembler = PromptAssembler::built_in();
        let prompt = assembler.assemble_package_prompt().unwrap();
        assert!(!prompt.is_empty());
    }

    #[test]
    fn asset_group_prompt_lists_all_assets() {
        use crate::codegen::AssetGroupItem;
        use crate::planning::PlanItem;

        let assembler = PromptAssembler::built_in();
        let temp = tempfile::TempDir::new().unwrap();
        let context = context(&temp);
        let request = AssetGroupRequest {
            project_root: PathBuf::from("E:/mods/m"),
            assets: vec![
                AssetGroupItem {
                    item: PlanItem {
                        id: "a1".into(),
                        item_type: "card".into(),
                        name: "FirstCard".into(),
                        description: "卡1".into(),
                        ..Default::default()
                    },
                    image_paths: vec![PathBuf::from("img/first.png")],
                },
                AssetGroupItem {
                    item: PlanItem {
                        id: "a2".into(),
                        item_type: "power".into(),
                        name: "SecondPower".into(),
                        description: "力1".into(),
                        ..Default::default()
                    },
                    image_paths: vec![],
                },
            ],
        };
        let prompt = assembler
            .assemble_asset_group_prompt(&request, &context)
            .unwrap();
        assert!(prompt.contains("FirstCard"));
        assert!(prompt.contains("SecondPower"));
        assert!(prompt.contains("`card`: locales eng, zhs; table `cards`"));
        assert!(prompt.contains("`power`: locales eng, zhs; table `powers`"));
        assert!(prompt.contains("卡1"));
        assert!(prompt.contains("code-only asset"));
        assert!(prompt.contains("first.png"));
    }
}
