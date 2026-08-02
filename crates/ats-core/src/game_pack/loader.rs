//! Strict loader for the stage-1 game-pack schema.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::error::{GamePackError, GamePackResult};
use super::model::{
    AssetResourceSpec, BuildLocalProperty, BuildRecipe, BuildRunner, BuildStep, GuidanceItem,
    GuidanceScenario, GuidanceSet, ImageResourceSpec, JsonManifestContract, LoadedGamePack,
    LocalizationResourceSpec, PackageLayout, ProjectTemplate, ProjectTemplateFile,
    ResourceImageRole, ResourceImageTransform, TruthSource, TruthSourceKind, ValidationRule,
};

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
        let source_name = manifest.display().to_string();
        let raw = parse_manifest(&source_name, &text)?;
        let template_files = raw
            .project_template
            .as_ref()
            .map(|template| load_template_from_dir(&source_name, pack_root, template))
            .transpose()?;
        let guidance_files = raw
            .guidance
            .as_ref()
            .map(|guidance| {
                load_resource_tree_from_dir(
                    &source_name,
                    pack_root,
                    &guidance.root,
                    "guidance.root",
                )
            })
            .transpose()?;
        self.validate(
            &source_name,
            raw,
            manifest_sha256(&text),
            template_files,
            guidance_files,
        )
    }

    /// Load an embedded Pack from an explicit, version-controlled resource file set.
    pub fn load_embedded_files<'a>(
        &self,
        source_name: &str,
        text: &str,
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> GamePackResult<LoadedGamePack> {
        let raw = parse_manifest(source_name, text)?;
        let embedded_files = files
            .into_iter()
            .map(|(relative_path, bytes)| {
                validate_relative_literal_path(
                    source_name,
                    "embedded_resources",
                    relative_path,
                    None,
                )?;
                Ok(ProjectTemplateFile {
                    relative_path: relative_path.into(),
                    bytes: bytes.to_vec(),
                })
            })
            .collect::<GamePackResult<Vec<_>>>()?;
        let template_files = raw
            .project_template
            .as_ref()
            .map(|template| {
                let files = embedded_files_under_root(
                    source_name,
                    &embedded_files,
                    &template.root,
                    "project_template.root",
                )?;
                select_declared_resource_files(
                    source_name,
                    "project_template.files",
                    files,
                    &template.files,
                )
            })
            .transpose()?;
        let guidance_files = raw
            .guidance
            .as_ref()
            .map(|guidance| {
                embedded_files_under_root(
                    source_name,
                    &embedded_files,
                    &guidance.root,
                    "guidance.root",
                )
            })
            .transpose()?;
        self.validate(
            source_name,
            raw,
            manifest_sha256(text),
            template_files,
            guidance_files,
        )
    }

    pub fn load_str(&self, source_name: &str, text: &str) -> GamePackResult<LoadedGamePack> {
        let raw = parse_manifest(source_name, text)?;
        self.validate(source_name, raw, manifest_sha256(text), None, None)
    }

    fn validate(
        &self,
        source_name: &str,
        raw: RawGamePack,
        content_sha256: String,
        template_files: Option<Vec<ProjectTemplateFile>>,
        guidance_files: Option<Vec<ProjectTemplateFile>>,
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

        let mut validation_rules = Vec::with_capacity(raw.validation_rules.len());
        let mut seen_rule_ids = BTreeSet::new();
        for (index, rule) in raw.validation_rules.into_iter().enumerate() {
            let prefix = format!("validation_rules[{index}]");
            validate_rule_id(source_name, &format!("{prefix}.id"), &rule.id)?;
            if !seen_rule_ids.insert(rule.id.clone()) {
                return invalid(
                    source_name,
                    format!("{prefix}.id"),
                    format!("duplicate validation rule id `{}`", rule.id),
                );
            }
            let validation_rule = match rule.kind.as_str() {
                "forbidden_call_in_method" => {
                    let method_name = required_identifier(
                        source_name,
                        &format!("{prefix}.method_name"),
                        rule.method_name,
                    )?;
                    let call_path = rule.call_path.ok_or_else(|| GamePackError::InvalidField {
                        source_name: source_name.to_string(),
                        field: format!("{prefix}.call_path"),
                        message: "is required for this validation rule kind".into(),
                    })?;
                    if call_path.is_empty() {
                        return invalid(
                            source_name,
                            format!("{prefix}.call_path"),
                            "must contain at least one identifier",
                        );
                    }
                    for (part_index, part) in call_path.iter().enumerate() {
                        validate_identifier(
                            source_name,
                            &format!("{prefix}.call_path[{part_index}]"),
                            part,
                        )?;
                    }
                    let message = required_non_empty(
                        source_name,
                        &format!("{prefix}.message"),
                        rule.message,
                    )?;
                    ValidationRule::ForbiddenCallInMethod {
                        id: rule.id,
                        method_name,
                        call_path,
                        message,
                    }
                }
                other => {
                    return invalid(
                        source_name,
                        format!("{prefix}.kind"),
                        format!("unknown validation rule kind `{other}`"),
                    );
                }
            };
            validation_rules.push(validation_rule);
        }
        let declares_validation = seen_capabilities.contains("validation_rules");
        if declares_validation && validation_rules.is_empty() {
            return invalid(
                source_name,
                "validation_rules",
                "capability `validation_rules` requires at least one rule",
            );
        }
        if !declares_validation && !validation_rules.is_empty() {
            return invalid(
                source_name,
                "validation_rules",
                "validation rule data requires capability `validation_rules`",
            );
        }

        let mut resource_specs = Vec::with_capacity(raw.resource_specs.len());
        let mut seen_resource_ids = BTreeSet::new();
        let mut seen_normalized_resource_ids = BTreeSet::new();
        for (index, resource) in raw.resource_specs.into_iter().enumerate() {
            let prefix = format!("resource_specs[{index}]");
            validate_id(source_name, &format!("{prefix}.id"), &resource.id)?;
            if !seen_resource_ids.insert(resource.id.clone()) {
                return invalid(
                    source_name,
                    format!("{prefix}.id"),
                    format!("duplicate resource spec id `{}`", resource.id),
                );
            }
            let normalized_id = normalize_asset_type(&resource.id);
            if !seen_normalized_resource_ids.insert(normalized_id) {
                return invalid(
                    source_name,
                    format!("{prefix}.id"),
                    "resource spec id collides after case/separator normalization",
                );
            }

            let localization_prefix = format!("{prefix}.localization");
            validate_id(
                source_name,
                &format!("{localization_prefix}.table"),
                &resource.localization.table,
            )?;
            validate_relative_pattern(
                source_name,
                &format!("{localization_prefix}.relative_path"),
                &resource.localization.relative_path,
                "{locale}",
                "json",
            )?;
            validate_unique_identifiers(
                source_name,
                &format!("{localization_prefix}.locales"),
                &resource.localization.locales,
            )?;
            validate_unique_identifiers(
                source_name,
                &format!("{localization_prefix}.required_suffixes"),
                &resource.localization.required_suffixes,
            )?;
            if !resource.localization.allowed_rich_text_tags.is_empty() {
                validate_unique_identifiers(
                    source_name,
                    &format!("{localization_prefix}.allowed_rich_text_tags"),
                    &resource.localization.allowed_rich_text_tags,
                )?;
            }

            let mut images = Vec::with_capacity(resource.images.len());
            let mut seen_roles = BTreeSet::new();
            for (image_index, image) in resource.images.into_iter().enumerate() {
                let image_prefix = format!("{prefix}.images[{image_index}]");
                let role = match image.role.as_str() {
                    "normal" => ResourceImageRole::Normal,
                    "outline" => ResourceImageRole::Outline,
                    "big" => ResourceImageRole::Big,
                    other => {
                        return invalid(
                            source_name,
                            format!("{image_prefix}.role"),
                            format!("unknown image role `{other}`"),
                        );
                    }
                };
                if !seen_roles.insert(role.as_str()) {
                    return invalid(
                        source_name,
                        format!("{image_prefix}.role"),
                        format!("duplicate image role `{}`", role.as_str()),
                    );
                }
                validate_relative_pattern(
                    source_name,
                    &format!("{image_prefix}.relative_path"),
                    &image.relative_path,
                    "{slug}",
                    "png",
                )?;
                let transform = match image.transform.as_str() {
                    "preserve" => {
                        reject_transform_field(
                            source_name,
                            &image_prefix,
                            "width",
                            image.width,
                            "preserve",
                        )?;
                        reject_transform_field(
                            source_name,
                            &image_prefix,
                            "height",
                            image.height,
                            "preserve",
                        )?;
                        reject_transform_field(
                            source_name,
                            &image_prefix,
                            "radius",
                            image.radius,
                            "preserve",
                        )?;
                        ResourceImageTransform::Preserve
                    }
                    "cover" => {
                        reject_transform_field(
                            source_name,
                            &image_prefix,
                            "radius",
                            image.radius,
                            "cover",
                        )?;
                        ResourceImageTransform::Cover {
                            width: required_positive_u32(
                                source_name,
                                &format!("{image_prefix}.width"),
                                image.width,
                            )?,
                            height: required_positive_u32(
                                source_name,
                                &format!("{image_prefix}.height"),
                                image.height,
                            )?,
                        }
                    }
                    "outline" => ResourceImageTransform::Outline {
                        width: required_positive_u32(
                            source_name,
                            &format!("{image_prefix}.width"),
                            image.width,
                        )?,
                        height: required_positive_u32(
                            source_name,
                            &format!("{image_prefix}.height"),
                            image.height,
                        )?,
                        radius: required_positive_u32(
                            source_name,
                            &format!("{image_prefix}.radius"),
                            image.radius,
                        )?,
                    },
                    other => {
                        return invalid(
                            source_name,
                            format!("{image_prefix}.transform"),
                            format!("unknown image transform `{other}`"),
                        );
                    }
                };
                images.push(ImageResourceSpec {
                    role,
                    relative_path: image.relative_path,
                    transform,
                });
            }

            resource_specs.push(AssetResourceSpec {
                id: resource.id,
                localization: LocalizationResourceSpec {
                    table: resource.localization.table,
                    relative_path: resource.localization.relative_path,
                    locales: resource.localization.locales,
                    required_suffixes: resource.localization.required_suffixes,
                    allowed_rich_text_tags: resource.localization.allowed_rich_text_tags,
                },
                images,
            });
        }
        let declares_resources = seen_capabilities.contains("resource_specs");
        if declares_resources && resource_specs.is_empty() {
            return invalid(
                source_name,
                "resource_specs",
                "capability `resource_specs` requires at least one resource spec",
            );
        }
        if !declares_resources && !resource_specs.is_empty() {
            return invalid(
                source_name,
                "resource_specs",
                "resource spec data requires capability `resource_specs`",
            );
        }

        let declares_guidance = seen_capabilities.contains("guidance");
        let guidance = match raw.guidance {
            Some(guidance) => {
                if !declares_guidance {
                    return invalid(
                        source_name,
                        "guidance",
                        "guidance data requires capability `guidance`",
                    );
                }
                validate_relative_literal_path(source_name, "guidance.root", &guidance.root, None)?;
                validate_sha256(source_name, "guidance.sha256", &guidance.sha256)?;
                let files = guidance_files.ok_or_else(|| GamePackError::InvalidField {
                    source_name: source_name.to_string(),
                    field: "guidance.root".into(),
                    message: "declared guidance resources are unavailable; load the Pack from a directory or embedded resource root".into(),
                })?;
                if files.is_empty() {
                    return invalid(
                        source_name,
                        "guidance.root",
                        "guidance directory must contain at least one file",
                    );
                }
                let actual_sha256 = template_tree_sha256(&files);
                if !actual_sha256.eq_ignore_ascii_case(&guidance.sha256) {
                    return invalid(
                        source_name,
                        "guidance.sha256",
                        format!(
                            "guidance tree checksum mismatch: expected {}, got {actual_sha256}",
                            guidance.sha256
                        ),
                    );
                }
                let file_map: BTreeMap<_, _> = files
                    .iter()
                    .map(|file| (file.relative_path.as_str(), file.bytes.as_slice()))
                    .collect();
                let mut seen_ids = BTreeSet::new();
                let mut referenced_files = BTreeSet::new();
                let mut items = Vec::with_capacity(guidance.items.len());
                for (index, item) in guidance.items.into_iter().enumerate() {
                    let prefix = format!("guidance.items[{index}]");
                    validate_id(source_name, &format!("{prefix}.id"), &item.id)?;
                    if !seen_ids.insert(item.id.clone()) {
                        return invalid(
                            source_name,
                            format!("{prefix}.id"),
                            format!("duplicate guidance item id `{}`", item.id),
                        );
                    }
                    validate_non_empty(source_name, &format!("{prefix}.title"), &item.title)?;
                    validate_relative_literal_path(
                        source_name,
                        &format!("{prefix}.relative_path"),
                        &item.relative_path,
                        None,
                    )?;
                    if Path::new(&item.relative_path)
                        .extension()
                        .and_then(|value| value.to_str())
                        != Some("md")
                    {
                        return invalid(
                            source_name,
                            format!("{prefix}.relative_path"),
                            "must point to a .md file",
                        );
                    }
                    if !referenced_files.insert(item.relative_path.clone()) {
                        return invalid(
                            source_name,
                            format!("{prefix}.relative_path"),
                            "guidance files must be referenced exactly once",
                        );
                    }
                    let bytes = file_map.get(item.relative_path.as_str()).ok_or_else(|| {
                        GamePackError::InvalidField {
                            source_name: source_name.to_string(),
                            field: format!("{prefix}.relative_path"),
                            message: "does not exist under guidance.root".into(),
                        }
                    })?;
                    let body = std::str::from_utf8(bytes)
                        .map_err(|error| GamePackError::InvalidField {
                            source_name: source_name.to_string(),
                            field: format!("{prefix}.relative_path"),
                            message: format!("guidance file is not UTF-8: {error}"),
                        })?
                        .trim()
                        .to_string();
                    if body.is_empty() {
                        return invalid(
                            source_name,
                            format!("{prefix}.relative_path"),
                            "guidance file must not be empty",
                        );
                    }
                    let mut seen_scenarios = BTreeSet::new();
                    let mut scenarios = Vec::with_capacity(item.scenarios.len());
                    for (scenario_index, scenario) in item.scenarios.iter().enumerate() {
                        let value = match scenario.as_str() {
                            "planner" => GuidanceScenario::Planner,
                            "asset_codegen" => GuidanceScenario::AssetCodegen,
                            "custom_code_codegen" => GuidanceScenario::CustomCodeCodegen,
                            "asset_group_codegen" => GuidanceScenario::AssetGroupCodegen,
                            other => {
                                return invalid(
                                    source_name,
                                    format!("{prefix}.scenarios[{scenario_index}]"),
                                    format!("unknown guidance scenario `{other}`"),
                                );
                            }
                        };
                        if !seen_scenarios.insert(value.as_str()) {
                            return invalid(
                                source_name,
                                format!("{prefix}.scenarios[{scenario_index}]"),
                                format!("duplicate guidance scenario `{scenario}`"),
                            );
                        }
                        scenarios.push(value);
                    }
                    if scenarios.is_empty() {
                        return invalid(
                            source_name,
                            format!("{prefix}.scenarios"),
                            "must contain at least one scenario",
                        );
                    }
                    validate_unique_identifiers(
                        source_name,
                        &format!("{prefix}.asset_types"),
                        &item.asset_types,
                    )
                    .or_else(|error| {
                        if item.asset_types.is_empty() {
                            Ok(())
                        } else {
                            Err(error)
                        }
                    })?;
                    items.push(GuidanceItem {
                        id: item.id,
                        title: item.title,
                        source_path: format!("pack://{}/guidance/{}", raw.id, item.relative_path),
                        scenarios,
                        asset_types: item.asset_types,
                        always: item.always,
                        body,
                    });
                }
                if items.is_empty() {
                    return invalid(
                        source_name,
                        "guidance.items",
                        "must contain at least one guidance item",
                    );
                }
                let all_files: BTreeSet<_> = file_map.keys().copied().collect();
                let referenced: BTreeSet<_> = referenced_files.iter().map(String::as_str).collect();
                if all_files != referenced {
                    return invalid(
                        source_name,
                        "guidance.items",
                        "every file under guidance.root must be referenced exactly once",
                    );
                }
                Some(GuidanceSet {
                    tree_sha256: actual_sha256,
                    items,
                })
            }
            None => {
                if declares_guidance {
                    return invalid(
                        source_name,
                        "guidance",
                        "capability `guidance` requires a guidance declaration",
                    );
                }
                None
            }
        };

        let declares_template = seen_capabilities.contains("project_template");
        let project_template = match raw.project_template {
            Some(template) => {
                if !declares_template {
                    return invalid(
                        source_name,
                        "project_template",
                        "project template data requires capability `project_template`",
                    );
                }
                validate_relative_literal_path(
                    source_name,
                    "project_template.root",
                    &template.root,
                    None,
                )?;
                validate_identifier(
                    source_name,
                    "project_template.placeholder",
                    &template.placeholder,
                )?;
                validate_sha256(source_name, "project_template.sha256", &template.sha256)?;
                let files = template_files.ok_or_else(|| GamePackError::InvalidField {
                    source_name: source_name.to_string(),
                    field: "project_template.root".into(),
                    message: "declared template resources are unavailable; load the Pack from a directory or embedded resource root".into(),
                })?;
                if files.is_empty() {
                    return invalid(
                        source_name,
                        "project_template.root",
                        "template directory must contain at least one file",
                    );
                }
                let actual_sha256 = template_tree_sha256(&files);
                if !actual_sha256.eq_ignore_ascii_case(&template.sha256) {
                    return invalid(
                        source_name,
                        "project_template.sha256",
                        format!(
                            "template tree checksum mismatch: expected {}, got {actual_sha256}",
                            template.sha256
                        ),
                    );
                }
                Some(ProjectTemplate {
                    placeholder: template.placeholder,
                    tree_sha256: actual_sha256,
                    files,
                })
            }
            None => {
                if declares_template {
                    return invalid(
                        source_name,
                        "project_template",
                        "capability `project_template` requires a template declaration",
                    );
                }
                None
            }
        };

        let declares_manifest = seen_capabilities.contains("manifest_contract");
        let manifest_contract = match raw.manifest_contract {
            Some(contract) => {
                if !declares_manifest {
                    return invalid(
                        source_name,
                        "manifest_contract",
                        "manifest contract data requires capability `manifest_contract`",
                    );
                }
                if project_template.is_none() {
                    return invalid(
                        source_name,
                        "manifest_contract",
                        "manifest contract requires capability `project_template` and a loaded template",
                    );
                }
                if !contract.relative_path.contains("{mod_id}") {
                    return invalid(
                        source_name,
                        "manifest_contract.relative_path",
                        "must contain the required `{mod_id}` placeholder",
                    );
                }
                validate_relative_literal_path(
                    source_name,
                    "manifest_contract.relative_path",
                    &contract.relative_path,
                    Some("{mod_id}"),
                )?;
                if Path::new(&contract.relative_path.replace("{mod_id}", "mod"))
                    .extension()
                    .and_then(|value| value.to_str())
                    != Some("json")
                {
                    return invalid(
                        source_name,
                        "manifest_contract.relative_path",
                        "must point to a .json file",
                    );
                }
                if !contract.expected.is_object() {
                    return invalid(
                        source_name,
                        "manifest_contract.expected",
                        "must be a JSON object",
                    );
                }
                validate_json_placeholders(
                    source_name,
                    "manifest_contract.expected",
                    &contract.expected,
                    "{mod_id}",
                )?;
                Some(JsonManifestContract {
                    relative_path: contract.relative_path,
                    expected: contract.expected,
                })
            }
            None => {
                if declares_manifest {
                    return invalid(
                        source_name,
                        "manifest_contract",
                        "capability `manifest_contract` requires a contract declaration",
                    );
                }
                None
            }
        };

        let declares_build = seen_capabilities.contains("build_recipe");
        let build_recipe = match raw.build_recipe {
            Some(recipe) => {
                if !declares_build {
                    return invalid(
                        source_name,
                        "build_recipe",
                        "build recipe data requires capability `build_recipe`",
                    );
                }
                let mut seen_inputs = BTreeSet::new();
                let mut seen_properties = BTreeSet::new();
                let mut local_properties = Vec::with_capacity(recipe.local_properties.len());
                for (index, property) in recipe.local_properties.into_iter().enumerate() {
                    let prefix = format!("build_recipe.local_properties[{index}]");
                    validate_id(
                        source_name,
                        &format!("{prefix}.input_key"),
                        &property.input_key,
                    )?;
                    validate_identifier(
                        source_name,
                        &format!("{prefix}.property"),
                        &property.property,
                    )?;
                    if !seen_inputs.insert(property.input_key.clone()) {
                        return invalid(
                            source_name,
                            format!("{prefix}.input_key"),
                            format!("duplicate local input key `{}`", property.input_key),
                        );
                    }
                    if !seen_properties.insert(property.property.clone()) {
                        return invalid(
                            source_name,
                            format!("{prefix}.property"),
                            format!("duplicate MSBuild property `{}`", property.property),
                        );
                    }
                    local_properties.push(BuildLocalProperty {
                        input_key: property.input_key,
                        property: property.property,
                    });
                }
                if local_properties.is_empty() {
                    return invalid(
                        source_name,
                        "build_recipe.local_properties",
                        "must contain at least one property mapping",
                    );
                }

                let mut seen_steps = BTreeSet::new();
                let mut steps = Vec::with_capacity(recipe.steps.len());
                for (index, step) in recipe.steps.into_iter().enumerate() {
                    let prefix = format!("build_recipe.steps[{index}]");
                    validate_id(source_name, &format!("{prefix}.id"), &step.id)?;
                    if !seen_steps.insert(step.id.clone()) {
                        return invalid(
                            source_name,
                            format!("{prefix}.id"),
                            format!("duplicate build step id `{}`", step.id),
                        );
                    }
                    let runner = match step.runner.as_str() {
                        "dotnet_publish" => BuildRunner::DotnetPublish,
                        other => {
                            return invalid(
                                source_name,
                                format!("{prefix}.runner"),
                                format!("unknown build runner `{other}`"),
                            );
                        }
                    };
                    steps.push(BuildStep {
                        id: step.id,
                        runner,
                    });
                }
                if steps.is_empty() {
                    return invalid(
                        source_name,
                        "build_recipe.steps",
                        "must contain at least one build step",
                    );
                }
                Some(BuildRecipe {
                    local_properties,
                    steps,
                })
            }
            None => {
                if declares_build {
                    return invalid(
                        source_name,
                        "build_recipe",
                        "capability `build_recipe` requires a recipe declaration",
                    );
                }
                None
            }
        };

        let declares_package = seen_capabilities.contains("package_layout");
        let package_layout = match raw.package_layout {
            Some(layout) => {
                if !declares_package {
                    return invalid(
                        source_name,
                        "package_layout",
                        "package layout data requires capability `package_layout`",
                    );
                }
                if layout.required_files.is_empty() {
                    return invalid(
                        source_name,
                        "package_layout.required_files",
                        "must contain at least one file",
                    );
                }
                let mut seen_files = BTreeSet::new();
                for (index, relative) in layout.required_files.iter().enumerate() {
                    let field = format!("package_layout.required_files[{index}]");
                    validate_relative_literal_path(
                        source_name,
                        &field,
                        relative,
                        Some("{mod_id}"),
                    )?;
                    let normalized = relative.replace('\\', "/");
                    if !seen_files.insert(normalized) {
                        return invalid(source_name, field, "duplicate package file path");
                    }
                }
                Some(PackageLayout {
                    required_files: layout.required_files,
                })
            }
            None => {
                if declares_package {
                    return invalid(
                        source_name,
                        "package_layout",
                        "capability `package_layout` requires a layout declaration",
                    );
                }
                None
            }
        };

        Ok(LoadedGamePack {
            schema_version: raw.schema_version,
            id: raw.id,
            content_sha256,
            display_name: raw.display_name,
            capabilities: raw.capabilities,
            truth_sources,
            validation_rules,
            resource_specs,
            guidance,
            project_template,
            manifest_contract,
            build_recipe,
            package_layout,
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
    #[serde(default)]
    validation_rules: Vec<RawValidationRule>,
    #[serde(default)]
    resource_specs: Vec<RawAssetResourceSpec>,
    guidance: Option<RawGuidanceSet>,
    project_template: Option<RawProjectTemplate>,
    manifest_contract: Option<RawJsonManifestContract>,
    build_recipe: Option<RawBuildRecipe>,
    package_layout: Option<RawPackageLayout>,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawValidationRule {
    id: String,
    kind: String,
    method_name: Option<String>,
    call_path: Option<Vec<String>>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAssetResourceSpec {
    id: String,
    localization: RawLocalizationResourceSpec,
    #[serde(default)]
    images: Vec<RawImageResourceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLocalizationResourceSpec {
    table: String,
    relative_path: String,
    locales: Vec<String>,
    required_suffixes: Vec<String>,
    #[serde(default)]
    allowed_rich_text_tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawImageResourceSpec {
    role: String,
    relative_path: String,
    transform: String,
    width: Option<u32>,
    height: Option<u32>,
    radius: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProjectTemplate {
    root: String,
    placeholder: String,
    sha256: String,
    files: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGuidanceSet {
    root: String,
    sha256: String,
    items: Vec<RawGuidanceItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGuidanceItem {
    id: String,
    title: String,
    relative_path: String,
    scenarios: Vec<String>,
    #[serde(default)]
    asset_types: Vec<String>,
    #[serde(default)]
    always: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJsonManifestContract {
    relative_path: String,
    expected: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuildRecipe {
    local_properties: Vec<RawBuildLocalProperty>,
    steps: Vec<RawBuildStep>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuildLocalProperty {
    input_key: String,
    property: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuildStep {
    id: String,
    runner: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackageLayout {
    required_files: Vec<String>,
}

fn parse_manifest(source_name: &str, text: &str) -> GamePackResult<RawGamePack> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|err| GamePackError::InvalidField {
        source_name: source_name.to_string(),
        field: json_path(err.path().to_string()),
        message: err.inner().to_string(),
    })
}

fn manifest_sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn load_template_from_dir(
    source_name: &str,
    pack_root: &Path,
    template: &RawProjectTemplate,
) -> GamePackResult<Vec<ProjectTemplateFile>> {
    let files = load_resource_tree_from_dir(
        source_name,
        pack_root,
        &template.root,
        "project_template.root",
    )?;
    select_declared_resource_files(
        source_name,
        "project_template.files",
        files,
        &template.files,
    )
}

fn load_resource_tree_from_dir(
    source_name: &str,
    pack_root: &Path,
    relative_root: &str,
    field: &str,
) -> GamePackResult<Vec<ProjectTemplateFile>> {
    validate_relative_literal_path(source_name, field, relative_root, None)?;
    let resource_root = resolve_pack_relative_path(pack_root, Path::new(relative_root))?;
    if !resource_root.is_dir() {
        return invalid(source_name, field, "must resolve to a directory");
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&resource_root).follow_links(false) {
        let entry = entry.map_err(|error| GamePackError::InvalidField {
            source_name: source_name.to_string(),
            field: field.into(),
            message: format!("cannot walk resource tree: {error}"),
        })?;
        if entry.path() == resource_root {
            continue;
        }
        if entry.file_type().is_symlink() {
            return invalid(
                source_name,
                field,
                format!(
                    "resource tree contains a symbolic link: {}",
                    entry.path().display()
                ),
            );
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return invalid(
                source_name,
                field,
                format!(
                    "resource tree contains a non-file entry: {}",
                    entry.path().display()
                ),
            );
        }
        let relative =
            entry
                .path()
                .strip_prefix(&resource_root)
                .map_err(|_| GamePackError::InvalidField {
                    source_name: source_name.to_string(),
                    field: field.into(),
                    message: format!("resource file escaped its root: {}", entry.path().display()),
                })?;
        let relative_path = normalized_relative_path(source_name, field, relative)?;
        let bytes = fs::read(entry.path()).map_err(|source| GamePackError::Read {
            path: entry.path().to_path_buf(),
            source,
        })?;
        files.push(ProjectTemplateFile {
            relative_path,
            bytes,
        });
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn select_declared_resource_files(
    source_name: &str,
    field: &str,
    files: Vec<ProjectTemplateFile>,
    declared: &[String],
) -> GamePackResult<Vec<ProjectTemplateFile>> {
    if declared.is_empty() {
        return invalid(source_name, field, "must contain at least one file path");
    }
    let mut available: BTreeMap<_, _> = files
        .into_iter()
        .map(|file| (file.relative_path.clone(), file))
        .collect();
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(declared.len());
    for (index, relative) in declared.iter().enumerate() {
        validate_relative_literal_path(source_name, &format!("{field}[{index}]"), relative, None)?;
        if !seen.insert(relative) {
            return invalid(
                source_name,
                format!("{field}[{index}]"),
                "duplicate resource file path",
            );
        }
        let file = available
            .remove(relative)
            .ok_or_else(|| GamePackError::InvalidField {
                source_name: source_name.to_string(),
                field: format!("{field}[{index}]"),
                message: "declared resource file does not exist".into(),
            })?;
        selected.push(file);
    }
    selected.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(selected)
}

fn embedded_files_under_root(
    source_name: &str,
    files: &[ProjectTemplateFile],
    relative_root: &str,
    field: &str,
) -> GamePackResult<Vec<ProjectTemplateFile>> {
    validate_relative_literal_path(source_name, field, relative_root, None)?;
    let prefix = format!("{relative_root}/");
    let mut selected = files
        .iter()
        .filter_map(|file| {
            file.relative_path
                .strip_prefix(&prefix)
                .map(|relative_path| ProjectTemplateFile {
                    relative_path: relative_path.into(),
                    bytes: file.bytes.clone(),
                })
        })
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if selected.is_empty() {
        return invalid(
            source_name,
            field,
            format!("embedded resource directory `{relative_root}` has no declared files"),
        );
    }
    Ok(selected)
}

fn normalized_relative_path(
    source_name: &str,
    field: &str,
    relative: &Path,
) -> GamePackResult<String> {
    let value = relative
        .to_str()
        .ok_or_else(|| GamePackError::InvalidField {
            source_name: source_name.to_string(),
            field: field.to_string(),
            message: format!("path is not UTF-8: {}", relative.display()),
        })?
        .replace('\\', "/");
    validate_relative_literal_path(source_name, field, &value, None)?;
    Ok(value)
}

fn template_tree_sha256(files: &[ProjectTemplateFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update([0]);
        hasher.update((file.bytes.len() as u64).to_le_bytes());
        hasher.update(Sha256::digest(&file.bytes));
    }
    format!("{:x}", hasher.finalize())
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

fn validate_rule_id(source_name: &str, field: &str, value: &str) -> GamePackResult<()> {
    validate_non_empty(source_name, field, value)?;
    if value
        .split('.')
        .all(|segment| !segment.is_empty() && validate_id(source_name, field, segment).is_ok())
    {
        Ok(())
    } else {
        invalid(
            source_name,
            field,
            "must be dot-separated lowercase identifier segments",
        )
    }
}

fn normalize_asset_type(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn validate_unique_identifiers(
    source_name: &str,
    field: &str,
    values: &[String],
) -> GamePackResult<()> {
    if values.is_empty() {
        return invalid(source_name, field, "must contain at least one identifier");
    }
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        validate_id(source_name, &format!("{field}[{index}]"), value)?;
        if !seen.insert(value) {
            return invalid(
                source_name,
                format!("{field}[{index}]"),
                format!("duplicate identifier `{value}`"),
            );
        }
    }
    Ok(())
}

fn validate_relative_pattern(
    source_name: &str,
    field: &str,
    value: &str,
    placeholder: &str,
    extension: &str,
) -> GamePackResult<()> {
    validate_non_empty(source_name, field, value)?;
    if !value.contains(placeholder) {
        return invalid(
            source_name,
            field,
            format!("must contain the required `{placeholder}` placeholder"),
        );
    }
    let without_placeholder = value.replace(placeholder, "value");
    if without_placeholder.contains(['{', '}']) {
        return invalid(source_name, field, "contains an unknown placeholder");
    }
    if value.contains('\\') {
        return invalid(source_name, field, "must use `/` path separators");
    }
    let path = Path::new(&without_placeholder);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid(
            source_name,
            field,
            "must be a relative path without `.` or `..` components",
        );
    }
    if path.extension().and_then(|value| value.to_str()) != Some(extension) {
        return invalid(
            source_name,
            field,
            format!("must use the `.{extension}` extension"),
        );
    }
    Ok(())
}

fn validate_relative_literal_path(
    source_name: &str,
    field: &str,
    value: &str,
    allowed_placeholder: Option<&str>,
) -> GamePackResult<()> {
    validate_non_empty(source_name, field, value)?;
    if value.contains('\\') {
        return invalid(source_name, field, "must use `/` path separators");
    }
    let normalized = if let Some(placeholder) = allowed_placeholder {
        value.replace(placeholder, "value")
    } else {
        value.to_string()
    };
    if normalized.contains(['{', '}']) {
        return invalid(source_name, field, "contains an unknown placeholder");
    }
    if value.starts_with('/')
        || value.ends_with('/')
        || value.split('/').any(str::is_empty)
        || Path::new(&normalized).is_absolute()
        || Path::new(&normalized)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid(
            source_name,
            field,
            "must be a normalized relative path without empty, `.` or `..` components",
        );
    }
    Ok(())
}

fn validate_json_placeholders(
    source_name: &str,
    field: &str,
    value: &serde_json::Value,
    allowed_placeholder: &str,
) -> GamePackResult<()> {
    match value {
        serde_json::Value::String(text) => {
            if text
                .replace(allowed_placeholder, "value")
                .contains(['{', '}'])
            {
                invalid(source_name, field, "contains an unknown placeholder")
            } else {
                Ok(())
            }
        }
        serde_json::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_json_placeholders(
                    source_name,
                    &format!("{field}[{index}]"),
                    value,
                    allowed_placeholder,
                )?;
            }
            Ok(())
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                if key
                    .replace(allowed_placeholder, "value")
                    .contains(['{', '}'])
                {
                    return invalid(
                        source_name,
                        format!("{field}.{key}"),
                        "contains an unknown placeholder",
                    );
                }
                validate_json_placeholders(
                    source_name,
                    &format!("{field}.{key}"),
                    value,
                    allowed_placeholder,
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn required_positive_u32(
    source_name: &str,
    field: &str,
    value: Option<u32>,
) -> GamePackResult<u32> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => invalid(source_name, field, "must be greater than zero"),
        None => invalid(source_name, field, "is required for this image transform"),
    }
}

fn reject_transform_field(
    source_name: &str,
    prefix: &str,
    field: &str,
    value: Option<u32>,
    transform: &str,
) -> GamePackResult<()> {
    if value.is_some() {
        invalid(
            source_name,
            format!("{prefix}.{field}"),
            format!("must be omitted for the `{transform}` transform"),
        )
    } else {
        Ok(())
    }
}

fn validate_identifier(source_name: &str, field: &str, value: &str) -> GamePackResult<()> {
    validate_non_empty(source_name, field, value)?;
    let mut chars = value.chars();
    let valid_start = chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
    if valid_start && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Ok(())
    } else {
        invalid(
            source_name,
            field,
            "must be an ASCII source identifier without path or regex syntax",
        )
    }
}

fn required_identifier(
    source_name: &str,
    field: &str,
    value: Option<String>,
) -> GamePackResult<String> {
    let value = required_non_empty(source_name, field, value)?;
    validate_identifier(source_name, field, &value)?;
    Ok(value)
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

    #[test]
    fn loads_finite_validation_rule_data() {
        let text = GOOD
            .replace(
                "\n      ]",
                r#"
      ],
      "validation_rules": [{
        "id": "fixture.energy.before_combat_start",
        "kind": "forbidden_call_in_method",
        "method_name": "BeforeCombatStart",
        "call_path": ["PlayerCmd", "GainEnergy"],
        "message": "ResetEnergy runs afterwards"
      }]"#,
            )
            .replace(
                "\"capabilities\": [\"truth_sources\"]",
                "\"capabilities\": [\"truth_sources\", \"validation_rules\"]",
            );
        let pack = loader().load_str("rules", &text).unwrap();
        assert!(matches!(
            &pack.validation_rules[0],
            ValidationRule::ForbiddenCallInMethod {
                id,
                method_name,
                call_path,
                ..
            } if id == "fixture.energy.before_combat_start"
                && method_name == "BeforeCombatStart"
                && call_path == &["PlayerCmd", "GainEnergy"]
        ));
    }

    #[test]
    fn rejects_unknown_or_unsafe_validation_rule_data() {
        let base = r#"{
          "schema_version": 1,
          "id": "test-game",
          "display_name": "Test Game",
          "capabilities": ["validation_rules"],
          "validation_rules": [{
            "id": "fixture.rule",
            "kind": "forbidden_call_in_method",
            "method_name": "BeforeCombatStart",
            "call_path": ["PlayerCmd", "GainEnergy"],
            "message": "known error"
          }]
        }"#;
        for (from, to, field) in [
            (
                "forbidden_call_in_method",
                "arbitrary_regex",
                "validation_rules[0].kind",
            ),
            (
                "BeforeCombatStart",
                "Before.*",
                "validation_rules[0].method_name",
            ),
            (
                "\"PlayerCmd\", \"GainEnergy\"",
                "\"PlayerCmd.GainEnergy(\"",
                "validation_rules[0].call_path[0]",
            ),
        ] {
            let error = loader()
                .load_str("bad-rule", &base.replace(from, to))
                .unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }

        let missing_capability = base.replace(
            "\"capabilities\": [\"validation_rules\"],",
            "\"capabilities\": [],",
        );
        let error = loader()
            .load_str("missing-capability", &missing_capability)
            .unwrap_err();
        assert!(error.to_string().contains("requires capability"));
    }

    #[test]
    fn loads_finite_resource_spec_and_normalizes_lookup() {
        let text = r#"{
          "schema_version": 1,
          "id": "test-game",
          "display_name": "Test Game",
          "capabilities": ["resource_specs"],
          "resource_specs": [{
            "id": "card_fullscreen",
            "localization": {
              "table": "cards",
              "relative_path": "localization/{locale}/cards.json",
              "locales": ["eng", "zhs"],
              "required_suffixes": ["title", "description"],
              "allowed_rich_text_tags": ["blue", "red"]
            },
            "images": [
              {"role":"normal", "relative_path":"images/cards/{slug}.png", "transform":"preserve"},
              {"role":"big", "relative_path":"images/cards/big/{slug}.png", "transform":"cover", "width":1024, "height":1024}
            ]
          }]
        }"#;
        let pack = loader().load_str("resources", text).unwrap();
        let spec = pack.resource_spec(" CARD-FULLSCREEN ").unwrap();
        assert_eq!(spec.localization.table, "cards");
        assert_eq!(spec.localization.allowed_rich_text_tags, ["blue", "red"]);
        assert_eq!(spec.images[1].role, ResourceImageRole::Big);
        assert!(matches!(
            spec.images[1].transform,
            ResourceImageTransform::Cover {
                width: 1024,
                height: 1024
            }
        ));
    }

    #[test]
    fn rejects_unsafe_or_unknown_resource_spec_data() {
        let base = r#"{
          "schema_version": 1,
          "id": "test-game",
          "display_name": "Test Game",
          "capabilities": ["resource_specs"],
          "resource_specs": [{
            "id": "relic",
            "localization": {
              "table": "relics",
              "relative_path": "localization/{locale}/relics.json",
              "locales": ["eng", "zhs"],
              "required_suffixes": ["title", "description", "flavor"],
              "allowed_rich_text_tags": ["blue", "red"]
            },
            "images": [{"role":"normal", "relative_path":"images/relics/{slug}.png", "transform":"cover", "width":128, "height":128}]
          }]
        }"#;
        for (from, to, field) in [
            (
                "localization/{locale}/relics.json",
                "../{locale}/relics.json",
                "resource_specs[0].localization.relative_path",
            ),
            (
                "images/relics/{slug}.png",
                "images/{asset}/relic.png",
                "resource_specs[0].images[0].relative_path",
            ),
            (
                "\"transform\":\"cover\"",
                "\"transform\":\"shell\"",
                "resource_specs[0].images[0].transform",
            ),
            (
                "\"allowed_rich_text_tags\": [\"blue\", \"red\"]",
                "\"allowed_rich_text_tags\": [\"blue\", \"blue\"]",
                "resource_specs[0].localization.allowed_rich_text_tags",
            ),
        ] {
            let error = loader()
                .load_str("bad-resource", &base.replace(from, to))
                .unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }

        let missing_capability = base.replace(
            "\"capabilities\": [\"resource_specs\"],",
            "\"capabilities\": [],",
        );
        let error = loader()
            .load_str("missing-capability", &missing_capability)
            .unwrap_err();
        assert!(error.to_string().contains("requires capability"));
    }

    #[test]
    fn loads_pack_owned_template_manifest_build_and_package_contracts() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack_root = temp.path().join("other-pack");
        let template_root = pack_root.join("template");
        fs::create_dir_all(&template_root).unwrap();
        let template_bytes = br#"{"id":"FixtureMod"}"#.to_vec();
        fs::write(template_root.join("FixtureMod.json"), &template_bytes).unwrap();
        let template_sha = template_tree_sha256(&[ProjectTemplateFile {
            relative_path: "FixtureMod.json".into(),
            bytes: template_bytes,
        }]);
        let manifest = serde_json::json!({
            "schema_version": 1,
            "id": "other-game",
            "display_name": "Other Game",
            "capabilities": [
                "project_template",
                "manifest_contract",
                "build_recipe",
                "package_layout"
            ],
            "project_template": {
                "root": "template",
                "placeholder": "FixtureMod",
                "sha256": template_sha,
                "files": ["FixtureMod.json"]
            },
            "manifest_contract": {
                "relative_path": "{mod_id}.json",
                "expected": {"id": "{mod_id}"}
            },
            "build_recipe": {
                "local_properties": [{
                    "input_key": "game_binary",
                    "property": "GameBinaryPath"
                }],
                "steps": [{"id": "publish", "runner": "dotnet_publish"}]
            },
            "package_layout": {
                "required_files": ["{mod_id}/{mod_id}.dll"]
            }
        });
        fs::write(
            pack_root.join(GAME_PACK_MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let pack = loader().load_from_dir(&pack_root).unwrap();
        assert_eq!(pack.id, "other-game");
        assert_eq!(pack.project_template.as_ref().unwrap().files.len(), 1);
        assert_eq!(
            pack.build_recipe.as_ref().unwrap().steps[0].runner,
            BuildRunner::DotnetPublish
        );
        assert_eq!(
            pack.package_layout.as_ref().unwrap().required_files,
            ["{mod_id}/{mod_id}.dll"]
        );
    }

    #[test]
    fn rejects_unavailable_template_unknown_runner_and_unsafe_package_path() {
        let template = r#"{
          "schema_version":1,
          "id":"other-game",
          "display_name":"Other Game",
          "capabilities":["project_template"],
          "project_template":{
            "root":"template",
            "placeholder":"FixtureMod",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "files":["FixtureMod.json"]
          }
        }"#;
        let error = loader().load_str("memory", template).unwrap_err();
        assert!(error.to_string().contains("resources are unavailable"));

        let build = r#"{
          "schema_version":1,
          "id":"other-game",
          "display_name":"Other Game",
          "capabilities":["build_recipe"],
          "build_recipe":{
            "local_properties":[{"input_key":"game_binary","property":"GameBinaryPath"}],
            "steps":[{"id":"publish","runner":"shell"}]
          }
        }"#;
        let error = loader().load_str("bad-runner", build).unwrap_err();
        assert!(error.to_string().contains("build_recipe.steps[0].runner"));

        let package = r#"{
          "schema_version":1,
          "id":"other-game",
          "display_name":"Other Game",
          "capabilities":["package_layout"],
          "package_layout":{"required_files":["../escape.dll"]}
        }"#;
        let error = loader().load_str("bad-package", package).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("package_layout.required_files[0]")
        );
    }

    #[test]
    fn rejects_unknown_guidance_scenario_and_checksum_mismatch() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack_root = temp.path().join("guidance-pack");
        let guidance_root = pack_root.join("guidance");
        fs::create_dir_all(&guidance_root).unwrap();
        let bytes = b"fixture guidance".to_vec();
        fs::write(guidance_root.join("common.md"), &bytes).unwrap();
        let sha = template_tree_sha256(&[ProjectTemplateFile {
            relative_path: "common.md".into(),
            bytes,
        }]);
        let manifest = |scenario: &str, checksum: &str| {
            serde_json::json!({
                "schema_version": 1,
                "id": "other-game",
                "display_name": "Other Game",
                "capabilities": ["guidance"],
                "guidance": {
                    "root": "guidance",
                    "sha256": checksum,
                    "items": [{
                        "id": "common",
                        "title": "Common",
                        "relative_path": "common.md",
                        "scenarios": [scenario],
                        "always": true
                    }]
                }
            })
        };
        fs::write(
            pack_root.join(GAME_PACK_MANIFEST),
            serde_json::to_vec_pretty(&manifest("shell", &sha)).unwrap(),
        )
        .unwrap();
        let error = loader().load_from_dir(&pack_root).unwrap_err();
        assert!(error.to_string().contains("guidance.items[0].scenarios[0]"));

        fs::write(
            pack_root.join(GAME_PACK_MANIFEST),
            serde_json::to_vec_pretty(&manifest(
                "asset_codegen",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ))
            .unwrap(),
        )
        .unwrap();
        let error = loader().load_from_dir(&pack_root).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("guidance tree checksum mismatch")
        );
    }
}
