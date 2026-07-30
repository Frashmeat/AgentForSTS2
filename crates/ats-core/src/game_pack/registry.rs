//! Registry for validated game packs available to the current application.

use std::collections::BTreeMap;

use super::error::{GamePackError, GamePackResult};
use super::loader::{GamePackLoadPolicy, GamePackLoader};
use super::model::LoadedGamePack;

const STS2_GAME_PACK: &str = include_str!("../../../../game_packs/sts2/game-pack.json");

#[derive(Debug, Clone, Default)]
pub struct GamePackRegistry {
    packs: BTreeMap<String, LoadedGamePack>,
}

impl GamePackRegistry {
    pub fn built_in() -> GamePackResult<Self> {
        let loader = GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project", "dotnet_file"],
            ["sts2_code_facts"],
        ));
        let sts2 = loader.load_str("built-in:sts2", STS2_GAME_PACK)?;
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

    #[must_use]
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
