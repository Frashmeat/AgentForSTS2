//! Registry for validated game packs available to the current application.

use std::collections::BTreeMap;

use super::error::{GamePackError, GamePackResult};
use super::loader::{GamePackLoadPolicy, GamePackLoader};
use super::model::LoadedGamePack;

const STS2_GAME_PACK: &str = include_str!("../../../../game_packs/sts2/game-pack.json");
const STS2_GAME_PACK_FILES: &[(&str, &[u8])] = &[
    (
        "guidance/card.md",
        include_bytes!("../../../../game_packs/sts2/guidance/card.md"),
    ),
    (
        "guidance/character.md",
        include_bytes!("../../../../game_packs/sts2/guidance/character.md"),
    ),
    (
        "guidance/common.md",
        include_bytes!("../../../../game_packs/sts2/guidance/common.md"),
    ),
    (
        "guidance/custom_code.md",
        include_bytes!("../../../../game_packs/sts2/guidance/custom_code.md"),
    ),
    (
        "guidance/planner_guidance.md",
        include_bytes!("../../../../game_packs/sts2/guidance/planner_guidance.md"),
    ),
    (
        "guidance/potion.md",
        include_bytes!("../../../../game_packs/sts2/guidance/potion.md"),
    ),
    (
        "guidance/power.md",
        include_bytes!("../../../../game_packs/sts2/guidance/power.md"),
    ),
    (
        "guidance/relic.md",
        include_bytes!("../../../../game_packs/sts2/guidance/relic.md"),
    ),
    (
        "template/.gitattributes",
        include_bytes!("../../../../game_packs/sts2/template/.gitattributes"),
    ),
    (
        "template/.gitignore",
        include_bytes!("../../../../game_packs/sts2/template/.gitignore"),
    ),
    (
        "template/Extensions/StringExtensions.cs",
        include_bytes!("../../../../game_packs/sts2/template/Extensions/StringExtensions.cs"),
    ),
    (
        "template/MainFile.cs",
        include_bytes!("../../../../game_packs/sts2/template/MainFile.cs"),
    ),
    (
        "template/ModTemplate.csproj",
        include_bytes!("../../../../game_packs/sts2/template/ModTemplate.csproj"),
    ),
    (
        "template/ModTemplate.json",
        include_bytes!("../../../../game_packs/sts2/template/ModTemplate.json"),
    ),
    (
        "template/ModTemplate.sln",
        include_bytes!("../../../../game_packs/sts2/template/ModTemplate.sln"),
    ),
    (
        "template/ModTemplate.sln.DotSettings",
        include_bytes!("../../../../game_packs/sts2/template/ModTemplate.sln.DotSettings"),
    ),
    (
        "template/export_presets.cfg",
        include_bytes!("../../../../game_packs/sts2/template/export_presets.cfg"),
    ),
    (
        "template/local.props.example",
        include_bytes!("../../../../game_packs/sts2/template/local.props.example"),
    ),
    (
        "template/nuget.config",
        include_bytes!("../../../../game_packs/sts2/template/nuget.config"),
    ),
    (
        "template/project.godot",
        include_bytes!("../../../../game_packs/sts2/template/project.godot"),
    ),
];

#[derive(Debug, Clone, Default)]
pub struct GamePackRegistry {
    packs: BTreeMap<String, LoadedGamePack>,
}

impl GamePackRegistry {
    pub fn built_in() -> GamePackResult<Self> {
        let loader = GamePackLoader::new(GamePackLoadPolicy::new(
            [
                "truth_sources",
                "validation_rules",
                "resource_specs",
                "guidance",
                "project_template",
                "manifest_contract",
                "build_recipe",
                "package_layout",
            ],
            ["dotnet_project", "dotnet_file"],
            ["sts2_code_facts"],
        ));
        let sts2 = loader.load_embedded_files(
            "built-in:sts2",
            STS2_GAME_PACK,
            STS2_GAME_PACK_FILES.iter().copied(),
        )?;
        Self::from_packs([sts2])
    }

    pub fn from_packs(packs: impl IntoIterator<Item = LoadedGamePack>) -> GamePackResult<Self> {
        let mut registry = Self::default();
        for pack in packs {
            registry.register(pack)?;
        }
        Ok(registry)
    }

    pub fn register(&mut self, pack: LoadedGamePack) -> GamePackResult<()> {
        if self.packs.contains_key(&pack.id) {
            return Err(GamePackError::DuplicatePackId { id: pack.id });
        }
        self.packs.insert(pack.id.clone(), pack);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&LoadedGamePack> {
        self.packs.get(id)
    }

    pub fn require(&self, id: &str) -> GamePackResult<&LoadedGamePack> {
        self.get(id)
            .ok_or_else(|| GamePackError::UnknownPackId { id: id.to_string() })
    }

    pub fn iter(&self) -> impl Iterator<Item = &LoadedGamePack> {
        self.packs.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_sts2_pack_is_loadable_and_pinned() {
        let registry = GamePackRegistry::built_in().unwrap();
        let sts2 = registry.require("sts2").unwrap();
        assert_eq!(sts2.display_name, "Slay the Spire 2");
        assert_eq!(sts2.truth_sources.len(), 2);
        assert!(matches!(
            &sts2.truth_sources[1].kind,
            super::super::TruthSourceKind::GitHubReleaseAsset {
                pinned_release,
                sha256,
                ..
            } if pinned_release == "v3.3.8"
                && sha256 == "e92213e9286cb8cb9db42b83735cc9ddc2d642a7c90c67c5350c983d734407a8"
        ));
        assert!(
            sts2.truth_sources
                .iter()
                .all(|source| source.provider == "sts2_code_facts")
        );
        assert_eq!(
            sts2.validation_rules[0].id(),
            "sts2.energy.before_combat_start"
        );
        let relic = sts2.resource_spec("Relic").unwrap();
        assert_eq!(relic.localization.table, "relics");
        assert!(
            relic
                .localization
                .allowed_rich_text_tags
                .iter()
                .any(|tag| tag == "blue")
        );
        assert!(
            !relic
                .localization
                .allowed_rich_text_tags
                .iter()
                .any(|tag| tag == "yellow")
        );
        assert_eq!(relic.images.len(), 3);
        assert!(sts2.project_template.is_some());
        assert_eq!(sts2.guidance.as_ref().unwrap().items.len(), 8);
        assert!(sts2.manifest_contract.is_some());
        assert_eq!(sts2.build_recipe.as_ref().unwrap().steps.len(), 1);
        assert_eq!(
            sts2.package_layout.as_ref().unwrap().required_files.len(),
            6
        );
    }

    #[test]
    fn duplicate_pack_ids_are_rejected() {
        let pack = GamePackRegistry::built_in()
            .unwrap()
            .require("sts2")
            .unwrap()
            .clone();
        let error = GamePackRegistry::from_packs([pack.clone(), pack]).unwrap_err();
        assert!(matches!(error, GamePackError::DuplicatePackId { .. }));
    }

    #[test]
    fn unknown_pack_id_is_rejected() {
        let error = GamePackRegistry::built_in()
            .unwrap()
            .require("unknown")
            .unwrap_err();
        assert!(matches!(error, GamePackError::UnknownPackId { .. }));
    }
}
