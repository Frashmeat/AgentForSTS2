use ats_kernel::Sha256Digest;
use thiserror::Error;

use crate::{LoadedGamePack, pack::BUILT_IN_STS2_SHA256};

const STS2_CHARACTER_DEFAULT_ID: &str = "character.default_identity";
const STS2_CHARACTER_DEFAULT: &[u8] =
    include_bytes!("../../../game_packs/sts2/resources/character-default-identity.png");

#[derive(Debug, Error, Eq, PartialEq)]
pub enum GamePackAssetError {
    #[error("game pack asset is not registered for this pinned Pack")]
    UnknownAsset,
}

pub fn built_in_game_pack_asset(
    pack: &LoadedGamePack,
    asset_id: &str,
) -> Result<&'static [u8], GamePackAssetError> {
    if pack.id().as_str() == "sts2"
        && pack.content_sha256()
            == &Sha256Digest::parse(BUILT_IN_STS2_SHA256)
                .expect("built-in STS2 Pack SHA-256 is valid")
        && asset_id == STS2_CHARACTER_DEFAULT_ID
    {
        Ok(STS2_CHARACTER_DEFAULT)
    } else {
        Err(GamePackAssetError::UnknownAsset)
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use crate::GamePackLoader;

    use super::*;

    #[test]
    fn built_in_asset_is_bound_to_the_exact_pack_identity() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let bytes = built_in_game_pack_asset(&pack, STS2_CHARACTER_DEFAULT_ID).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            "ec1b4f15aeb864166e0ec60936acfa390de9a0e9f2e06e41c5dc5a729a34c393"
        );
        assert!(matches!(
            built_in_game_pack_asset(&pack, "character.unknown"),
            Err(GamePackAssetError::UnknownAsset)
        ));
    }
}
