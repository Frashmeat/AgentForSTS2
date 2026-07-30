//! Strict loader for the stage-1 game-pack schema.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::error::{GamePackError, GamePackResult};
use super::model::{LoadedGamePack, TruthSource, TruthSourceKind};

pub const GAME_PACK_SCHEMA_VERSION: u32 = 1;
const GAME_PACK_MANIFEST: &str = "game-pack.json";

#[derive(Debug, Clone, Default)]
pub struct GamePackLoadPolicy {
    capabilities: BTreeSet<String>,
    indexers: BTreeSet<String>,
    providers: BTreeSet<String>,
}

impl GamePackLoadPolicy {
    pub fn new(
        capabilities: impl IntoIterator<Item = impl Into<String>>,
        indexers: impl IntoIterator<Item = impl Into<String>>,
        providers: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            capabilities: capabilities.into_iter().map(Into::into).collect(),
            indexers: indexers.into_iter().map(Into::into).collect(),
            providers: providers.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GamePackLoader {
    policy: GamePackLoadPolicy,
}

impl GamePackLoader {
    #[must_use]
    pub fn new(policy: GamePackLoadPolicy) -> Self {
        Self { policy }
    }

    pub fn load_from_dir(&self, pack_root: &Path) -> GamePackResult<LoadedGamePack> {
        let manifest = pack_root.join(GAME_PACK_MANIFEST);
        let text = fs::read_to_string(&manifest).map_err(|source| GamePackError::Read {
            path: manifest.clone(),
            source,
        })?;
        self.load_str(&manifest.display().to_string(), &text)
    }

    pub fn load_str(&self, source_name: &str, text: &str) -> GamePackResult<LoadedGamePack> {
        let content_sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let raw: RawGamePack =
            serde_path_to_error::deserialize(&mut deserializer).map_err(|err| {
                GamePackError::InvalidField {
                    source_name: source_name.to_string(),
                    field: json_path(err.path().to_string()),
                    message: err.inner().to_string(),
                }
            })?;
        self.validate(source_name, raw, content_sha256)
    }

    fn validate(
        &self,
        source_name: &str,
        raw: RawGamePack,
        content_sha256: String,
    ) -> GamePackResult<LoadedGamePack> {
        if raw.schema_version != GAME_PACK_SCHEMA_VERSION {
            return Err(GamePackError::UnsupportedSchema {
                source_name: source_name.to_string(),
                found: raw.schema_version,
                supported: GAME_PACK_SCHEMA_VERSION,
            });
        }
        validate_id(source_name, "id", &raw.id)?;
        validate_non_empty(source_name, "display_name", &raw.display_name)?;

        let mut seen_capabilities = BTreeSet::new();
        for (index, capability) in raw.capabilities.iter().enumerate() {
            let field = format!("capabilities[{index}]");
            validate_id(source_name, &field, capability)?;
            if !self.policy.capabilities.contains(capability) {
                return invalid(
                    source_name,
                    field,
                    format!("unknown capability `{capability}`"),
                );
            }
            if !seen_capabilities.insert(capability.as_str()) {
                return invalid(
                    source_name,
                    field,
                    format!("duplicate capability `{capability}`"),
                );
            }
        }

        let mut truth_sources = Vec::with_capacity(raw.truth_sources.len());
        let mut seen_source_ids = BTreeSet::new();
        for (index, source) in raw.truth_sources.into_iter().enumerate() {
            let prefix = format!("truth_sources[{index}]");
            validate_id(source_name, &format!("{prefix}.id"), &source.id)?;
            if !seen_source_ids.insert(source.id.clone()) {
                return Err(GamePackError::DuplicateTruthSourceId {
                    source_name: source_name.to_string(),
                    id: source.id,
                    index,
                });
            }
            validate_supported(
                source_name,
                &format!("{prefix}.indexer"),
                &source.indexer,
                "indexer",
                &self.policy.indexers,
            )?;
            validate_supported(
                source_name,
                &format!("{prefix}.provider"),
                &source.provider,
                "provider",
                &self.policy.providers,
            )?;
            let kind = match source.kind.as_str() {
                "local_file" => TruthSourceKind::LocalFile {
                    input_key: required_id(
                        source_name,
                        &format!("{prefix}.input_key"),
                        source.input_key,
                    )?,
                },
                "github_release_asset" => {
                    let repository = required_non_empty(
                        source_name,
                        &format!("{prefix}.repository"),
                        source.repository,
                    )?;
                    validate_repository(source_name, &format!("{prefix}.repository"), &repository)?;
                    let pinned_release = required_non_empty(
                        source_name,
                        &format!("{prefix}.pinned_release"),
                        source.pinned_release,
                    )?;
                    let asset =
                        required_non_empty(source_name, &format!("{prefix}.asset"), source.asset)?;
                    if asset.contains(['/', '\\'])
                        || Path::new(&asset).file_name().and_then(|part| part.to_str())
                            != Some(asset.as_str())
                    {
                        return invalid(
                            source_name,
                            format!("{prefix}.asset"),
                            "asset must be a file name without path components",
                        );
                    }
                    let sha256 = required_non_empty(
                        source_name,
                        &format!("{prefix}.sha256"),
                        source.sha256,
                    )?;
                    validate_sha256(source_name, &format!("{prefix}.sha256"), &sha256)?;
                    TruthSourceKind::GitHubReleaseAsset {
                        repository,
                        pinned_release,
                        asset,
                        sha256: sha256.to_ascii_lowercase(),
                    }
                }
                other => {
                    return invalid(
                        source_name,
                        format!("{prefix}.kind"),
                        format!("unknown truth source kind `{other}`"),
                    );
                }
            };
            truth_sources.push(TruthSource {
                id: source.id,
                kind,
                indexer: source.indexer,
                provider: source.provider,
            });
        }

        if seen_capabilities.contains("truth_sources") && truth_sources.is_empty() {
            return invalid(
                source_name,
                "truth_sources",
                "capability `truth_sources` requires at least one source",
            );
        }

        Ok(LoadedGamePack {
            schema_version: raw.schema_version,
            id: raw.id,
            content_sha256,
            display_name: raw.display_name,
            capabilities: raw.capabilities,
            truth_sources,
        })
    }
}

/// Resolve an existing pack-relative file and prove it remains inside `pack_root`.
/// Future resource/template declarations must pass through this boundary.
pub fn resolve_pack_relative_path(pack_root: &Path, relative: &Path) -> GamePackResult<PathBuf> {
    let root = fs::canonicalize(pack_root).map_err(|source| GamePackError::Read {
        path: pack_root.to_path_buf(),
        source,
    })?;
    let candidate = if relative.is_absolute() {
        relative.to_path_buf()
    } else {
        root.join(relative)
    };
    let resolved = fs::canonicalize(&candidate).map_err(|source| GamePackError::Read {
        path: candidate,
        source,
    })?;
    if !resolved.starts_with(&root) {
        return Err(GamePackError::PathOutsideRoot {
            root,
            relative: relative.to_path_buf(),
            resolved,
        });
    }
    Ok(resolved)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGamePack {
    schema_version: u32,
    id: String,
    display_name: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    truth_sources: Vec<RawTruthSource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTruthSource {
    id: String,
    kind: String,
    indexer: String,
    provider: String,
    input_key: Option<String>,
    repository: Option<String>,
    pinned_release: Option<String>,
    asset: Option<String>,
    sha256: Option<String>,
}

fn json_path(path: String) -> String {
    if path.is_empty() { "$".into() } else { path }
}

fn invalid<T>(
    source_name: &str,
    field: impl Into<String>,
    message: impl Into<String>,
) -> GamePackResult<T> {
    Err(GamePackError::InvalidField {
        source_name: source_name.to_string(),
        field: field.into(),
        message: message.into(),
    })
}

fn validate_non_empty(source_name: &str, field: &str, value: &str) -> GamePackResult<()> {
    if value.trim().is_empty() {
        invalid(source_name, field, "must not be empty")
    } else if value != value.trim() {
        invalid(
            source_name,
            field,
            "must not have leading or trailing whitespace",
        )
    } else {
        Ok(())
    }
}

fn validate_id(source_name: &str, field: &str, value: &str) -> GamePackResult<()> {
    validate_non_empty(source_name, field, value)?;
    if value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        Ok(())
    } else {
        invalid(
            source_name,
            field,
            "must start with a lowercase ASCII letter or digit and contain only lowercase ASCII letters, digits, `_`, or `-`",
        )
    }
}

fn required_non_empty(
    source_name: &str,
    field: &str,
    value: Option<String>,
) -> GamePackResult<String> {
    let value = value.ok_or_else(|| GamePackError::InvalidField {
        source_name: source_name.to_string(),
        field: field.to_string(),
        message: "is required for this truth source kind".into(),
    })?;
    validate_non_empty(source_name, field, &value)?;
    Ok(value)
}

fn required_id(source_name: &str, field: &str, value: Option<String>) -> GamePackResult<String> {
    let value = required_non_empty(source_name, field, value)?;
    validate_id(source_name, field, &value)?;
    Ok(value)
}

fn validate_supported(
    source_name: &str,
    field: &str,
    value: &str,
    kind: &str,
    supported: &BTreeSet<String>,
) -> GamePackResult<()> {
    validate_id(source_name, field, value)?;
    if supported.contains(value) {
        Ok(())
    } else {
        invalid(source_name, field, format!("unknown {kind} `{value}`"))
    }
}

fn validate_repository(source_name: &str, field: &str, value: &str) -> GamePackResult<()> {
    let mut parts = value.split('/');
    let valid_segment =
        |part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains('\\');
    let valid = parts.next().is_some_and(valid_segment)
        && parts.next().is_some_and(valid_segment)
        && parts.next().is_none()
        && !value.starts_with('/');
    if valid {
        Ok(())
    } else {
        invalid(
            source_name,
            field,
            "must be an `owner/repository` identifier",
        )
    }
}

fn validate_sha256(source_name: &str, field: &str, value: &str) -> GamePackResult<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        invalid(
            source_name,
            field,
            "must contain exactly 64 hexadecimal SHA-256 characters",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
      "schema_version": 1,
      "id": "test-game",
      "display_name": "Test Game",
      "capabilities": ["truth_sources"],
      "truth_sources": [
        {
          "id": "game",
          "kind": "local_file",
          "input_key": "game_assembly",
          "indexer": "dotnet_project",
          "provider": "test_code_facts"
        }
      ]
    }"#;

    fn loader() -> GamePackLoader {
        GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project", "dotnet_file"],
            ["test_code_facts"],
        ))
    }

    #[test]
    fn loads_valid_pack() {
        let pack = loader().load_str("fixture", GOOD).unwrap();
        assert_eq!(pack.id, "test-game");
        assert_eq!(pack.content_sha256.len(), 64);
        assert_eq!(pack.truth_sources[0].provider, "test_code_facts");
    }

    #[test]
    fn defaults_optional_collections() {
        let pack = loader()
            .load_str(
                "base",
                r#"{"schema_version":1,"id":"empty-game","display_name":"Empty Game"}"#,
            )
            .unwrap();
        assert!(pack.capabilities.is_empty());
        assert!(pack.truth_sources.is_empty());
    }

    #[test]
    fn rejects_unknown_schema_and_capability() {
        let error = loader()
            .load_str(
                "bad-schema",
                &GOOD.replace("\"schema_version\": 1", "\"schema_version\": 2"),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            GamePackError::UnsupportedSchema { found: 2, .. }
        ));

        let error = loader()
            .load_str(
                "bad-capability",
                &GOOD.replace(
                    "\"capabilities\": [\"truth_sources\"]",
                    "\"capabilities\": [\"shell\"]",
                ),
            )
            .unwrap_err();
        assert!(error.to_string().contains("capabilities[0]"));
    }

    #[test]
    fn rejects_unknown_kind_indexer_and_provider() {
        for (from, to, field) in [
            ("local_file", "command", "truth_sources[0].kind"),
            ("dotnet_project", "shell", "truth_sources[0].indexer"),
            (
                "test_code_facts",
                "unknown_provider",
                "truth_sources[0].provider",
            ),
        ] {
            let error = loader()
                .load_str("bad", &GOOD.replace(from, to))
                .unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }
    }

    #[test]
    fn rejects_missing_kind_field_and_duplicate_source_id() {
        let github = GOOD
            .replace("local_file", "github_release_asset")
            .replace("\n          \"input_key\": \"game_assembly\",", "");
        let error = loader().load_str("missing", &github).unwrap_err();
        assert!(error.to_string().contains("truth_sources[0].repository"));

        let duplicate = GOOD.replace(
            "\n      ]",
            r#",
        {
          "id": "game",
          "kind": "local_file",
          "input_key": "second_assembly",
          "indexer": "dotnet_file",
          "provider": "test_code_facts"
        }
      ]"#,
        );
        let error = loader().load_str("duplicate", &duplicate).unwrap_err();
        assert!(matches!(
            error,
            GamePackError::DuplicateTruthSourceId { .. }
        ));
    }

    #[test]
    fn github_release_asset_requires_valid_sha256() {
        let github = GOOD
            .replace("local_file", "github_release_asset")
            .replace(
                "\"input_key\": \"game_assembly\"",
                "\"repository\": \"owner/repo\", \"pinned_release\": \"v1\", \"asset\": \"library.dll\"",
            );
        let error = loader().load_str("missing-sha", &github).unwrap_err();
        assert!(error.to_string().contains("truth_sources[0].sha256"));

        let invalid = github.replace(
            "\"asset\": \"library.dll\"",
            "\"asset\": \"library.dll\", \"sha256\": \"not-a-digest\"",
        );
        let error = loader().load_str("invalid-sha", &invalid).unwrap_err();
        assert!(error.to_string().contains("64 hexadecimal"));
    }

    #[test]
    fn rejects_invalid_ids_and_path_escape() {
        let error = loader()
            .load_str("bad-id", &GOOD.replace("test-game", "../escape"))
            .unwrap_err();
        assert!(error.to_string().contains("`id`"));

        let parent = tempfile::TempDir::new().unwrap();
        let root = parent.path().join("pack");
        fs::create_dir(&root).unwrap();
        let outside = parent.path().join("outside.txt");
        fs::write(&outside, "outside").unwrap();
        let error = resolve_pack_relative_path(&root, Path::new("../outside.txt")).unwrap_err();
        assert!(matches!(error, GamePackError::PathOutsideRoot { .. }));
    }
}
