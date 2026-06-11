//! Codegen prompt 装配——把请求 + 知识包渲染成完整 LLM prompt。
//!
//! 与 Python 端的简化：移除了 "legacy fallback"（当 resolver 缺失/返回空时回退到
//! Sts2GuidanceKnowledgeSource）。Rust 端 resolver 永远可用，guidance 一定非空
//! （内嵌模板兜底），所以裸跑 resolver 即可。Facts 当前为 stub（stage 2.2 之前为空），
//! 我们在 prompt 中明确告知 LLM "结构化代码事实暂不可用，请直接读源码"。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::codegen::models::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
};
use crate::knowledge::{
    KnowledgePacket, KnowledgePaths, KnowledgeQuery, KnowledgeScenario, SourceMode,
    Sts2KnowledgeResolver,
};
use crate::prompting::{PromptContextAssembler, PromptError, PromptLoader};

const GAME_API_REFERENCE_FILE_NAME: &str = "sts2_api_reference.md";

const FACTS_STUB_MESSAGE: &str = "Structured code facts are not yet available in the Rust port (stage 2.2 todo). \
Read `MainFile.cs`, the project `.csproj`, and the lookup sources below to verify exact base classes, \
signatures, resource paths, and registration behavior before writing code.";

const FACTS_STUB_WARNING: &str = "### Warnings\n- Structured code facts are unavailable in this build. \
Treat the guidance summary as best-effort context, not as the authoritative code fact source.";

pub struct PromptAssembler {
    pub loader: PromptLoader,
    pub resolver: Sts2KnowledgeResolver,
    pub context_assembler: PromptContextAssembler,
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
        paths: &KnowledgePaths,
        game_source_mode: SourceMode,
    ) -> Result<String, PromptError> {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            domain: "sts2".into(),
            asset_type: Some(request.asset_type.clone()),
            project_root: Some(request.project_root.clone()),
            requirements: Some(request.design_description.clone()),
            item_name: Some(request.asset_name.clone()),
            symbols: Vec::new(),
            group_asset_types: Vec::new(),
        };
        let knowledge = self.resolve_knowledge(&query, paths, game_source_mode);

        let img_list = format_image_list(&request.image_paths);
        let zhs_hint = if request.name_zhs.is_empty() {
            String::new()
        } else {
            format!(
                "\nSimplified Chinese display name (name_zhs): {}",
                request.name_zhs
            )
        };
        let build_note = "NOTE: Godot headless export always exits with code -1, but if MSBuild reports '0 Error(s)' and the overall dotnet exit code is 0 — that is SUCCESS. Do NOT re-run just because of Godot's -1.";
        let build_step = if request.skip_build {
            "6. Do NOT run dotnet publish — the build will be done later after all assets are created.".to_string()
        } else {
            format!(
                "6. Run `dotnet publish` (NOT dotnet build) to compile AND export the Godot .pck file.\n   {build_note}\n   Fix any actual compilation errors and re-run until it succeeds.\n7. Confirm both the .dll and .pck were deployed to the mods folder."
            )
        };

        let api_ref = api_ref_path_string(paths);
        let project_root = path_to_posix(&request.project_root);
        let mod_name = path_basename(&request.project_root);

        let vars = HashMap::from([
            ("api_ref_path", api_ref.as_str()),
            ("asset_name", request.asset_name.as_str()),
            ("asset_type", request.asset_type.as_str()),
            ("build_step", build_step.as_str()),
            ("design_description", request.design_description.as_str()),
            ("facts", knowledge.facts.as_str()),
            ("guidance", knowledge.guidance.as_str()),
            ("lookup", knowledge.lookup.as_str()),
            ("knowledge_warnings", knowledge.warnings.as_str()),
            ("img_list", img_list.as_str()),
            ("mod_name", mod_name.as_str()),
            ("project_root", project_root.as_str()),
            ("zhs_hint", zhs_hint.as_str()),
        ]);
        self.loader.render("codegen.asset_prompt", &vars)
    }

    pub fn assemble_custom_code_prompt(
        &self,
        request: &CustomCodegenRequest,
        paths: &KnowledgePaths,
        game_source_mode: SourceMode,
    ) -> Result<String, PromptError> {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::CustomCodeCodegen),
            domain: "sts2".into(),
            asset_type: Some("custom_code".into()),
            project_root: Some(request.project_root.clone()),
            requirements: Some(request.description.clone()),
            item_name: Some(request.name.clone()),
            symbols: Vec::new(),
            group_asset_types: Vec::new(),
        };
        let knowledge = self.resolve_knowledge(&query, paths, game_source_mode);

        let build_note = "NOTE: Godot headless export always exits with code -1, but if MSBuild reports '0 Error(s)' and the overall dotnet exit code is 0 — that is SUCCESS. Do NOT re-run just because of Godot's -1.";
        let build_steps = if request.skip_build {
            "5. Do NOT run dotnet publish — the build will be done later after all assets are created.".to_string()
        } else {
            format!(
                "5. Run `dotnet publish` (NOT dotnet build). {build_note}\n   Fix any actual compilation errors and re-run until it succeeds.\n6. Confirm the build succeeded and files deployed to mods folder."
            )
        };

        let api_ref = api_ref_path_string(paths);
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
        self.loader.render("codegen.custom_code_prompt", &vars)
    }

    pub fn assemble_asset_group_prompt(
        &self,
        request: &AssetGroupRequest,
        paths: &KnowledgePaths,
        game_source_mode: SourceMode,
    ) -> Result<String, PromptError> {
        let symbols: Vec<String> = request.assets.iter().map(|a| a.item.name.clone()).collect();
        let group_asset_types: Vec<String> = request
            .assets
            .iter()
            .map(|a| asset_type_str(&a.item.item_type).into())
            .collect();
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetGroupCodegen),
            domain: "sts2".into(),
            asset_type: None,
            project_root: Some(request.project_root.clone()),
            requirements: None,
            item_name: None,
            symbols,
            group_asset_types,
        };
        let knowledge = self.resolve_knowledge(&query, paths, game_source_mode);

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

        let vars = HashMap::from([
            ("asset_count", asset_count.as_str()),
            ("assets_section", assets_section.as_str()),
            ("class_names", class_names.as_str()),
            ("facts", knowledge.facts.as_str()),
            ("guidance", knowledge.guidance.as_str()),
            ("lookup", knowledge.lookup.as_str()),
            ("knowledge_warnings", knowledge.warnings.as_str()),
            ("mod_name", mod_name.as_str()),
            ("project_root", project_root.as_str()),
        ]);
        self.loader.render("codegen.asset_group_prompt", &vars)
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
        paths: &KnowledgePaths,
        game_source_mode: SourceMode,
    ) -> ResolvedKnowledge {
        let packet = self.resolver.resolve(query, paths, game_source_mode);
        ResolvedKnowledge::from_packet(&packet, &self.context_assembler)
    }
}

struct ResolvedKnowledge {
    facts: String,
    guidance: String,
    lookup: String,
    warnings: String,
}

impl ResolvedKnowledge {
    fn from_packet(packet: &KnowledgePacket, assembler: &PromptContextAssembler) -> Self {
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
        }
    }
}

fn asset_type_str(asset_type: &crate::planning::AssetItemType) -> &'static str {
    use crate::planning::AssetItemType::*;
    match asset_type {
        Card => "card",
        CardFullscreen => "card_fullscreen",
        Relic => "relic",
        Power => "power",
        Character => "character",
        CustomCode => "custom_code",
    }
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

fn api_ref_path_string(paths: &KnowledgePaths) -> String {
    path_to_posix(&paths.game_dir.join(GAME_API_REFERENCE_FILE_NAME))
}

fn path_to_posix(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn path_basename(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> KnowledgePaths {
        KnowledgePaths::from_runtime_dir(Path::new("/tmp/runtime"))
    }

    #[test]
    fn asset_prompt_contains_required_fragments() {
        let assembler = PromptAssembler::built_in();
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
            .assemble_asset_prompt(&request, &paths(), SourceMode::Missing)
            .expect("assembly should succeed");
        assert!(prompt.contains("DemoCard"));
        assert!(prompt.contains("card"));
        assert!(prompt.contains("演示卡"));
        assert!(prompt.contains("demo.png"));
        assert!(prompt.contains("dotnet publish"));
        // facts stub injected when game source missing
        assert!(prompt.contains("Structured code facts are not yet available"));
    }

    #[test]
    fn asset_prompt_skip_build_omits_publish() {
        let assembler = PromptAssembler::built_in();
        let request = AssetCodegenRequest {
            asset_type: "card".into(),
            asset_name: "X".into(),
            project_root: PathBuf::from("E:/mods/m"),
            skip_build: true,
            ..Default::default()
        };
        let prompt = assembler
            .assemble_asset_prompt(&request, &paths(), SourceMode::Missing)
            .unwrap();
        assert!(prompt.contains("Do NOT run dotnet publish"));
        assert!(!prompt.contains("Run `dotnet publish` (NOT dotnet build) to compile"));
    }

    #[test]
    fn custom_code_prompt_renders() {
        let assembler = PromptAssembler::built_in();
        let request = CustomCodegenRequest {
            name: "MyHook".into(),
            description: "钩子描述".into(),
            implementation_notes: "实现要点".into(),
            project_root: PathBuf::from("E:/mods/m"),
            skip_build: false,
        };
        let prompt = assembler
            .assemble_custom_code_prompt(&request, &paths(), SourceMode::Missing)
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
        use crate::planning::{AssetItemType, PlanItem};

        let assembler = PromptAssembler::built_in();
        let request = AssetGroupRequest {
            project_root: PathBuf::from("E:/mods/m"),
            assets: vec![
                AssetGroupItem {
                    item: PlanItem {
                        id: "a1".into(),
                        item_type: AssetItemType::Card,
                        name: "FirstCard".into(),
                        description: "卡1".into(),
                        ..Default::default()
                    },
                    image_paths: vec![PathBuf::from("img/first.png")],
                },
                AssetGroupItem {
                    item: PlanItem {
                        id: "a2".into(),
                        item_type: AssetItemType::Power,
                        name: "SecondPower".into(),
                        description: "力1".into(),
                        ..Default::default()
                    },
                    image_paths: vec![],
                },
            ],
        };
        let prompt = assembler
            .assemble_asset_group_prompt(&request, &paths(), SourceMode::Missing)
            .unwrap();
        assert!(prompt.contains("FirstCard"));
        assert!(prompt.contains("SecondPower"));
        assert!(prompt.contains("卡1"));
        assert!(prompt.contains("code-only asset"));
        assert!(prompt.contains("first.png"));
    }
}
