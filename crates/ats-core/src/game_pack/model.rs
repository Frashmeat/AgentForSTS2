//! Validated game-pack model exposed to consumers.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedGamePack {
    pub schema_version: u32,
    pub id: String,
    /// SHA-256 of the exact manifest bytes accepted by the loader.
    pub content_sha256: String,
    pub display_name: String,
    pub capabilities: Vec<String>,
    pub truth_sources: Vec<TruthSource>,
    pub validation_rules: Vec<ValidationRule>,
    pub resource_specs: Vec<AssetResourceSpec>,
    pub guidance: Option<GuidanceSet>,
    pub project_template: Option<ProjectTemplate>,
    pub manifest_contract: Option<JsonManifestContract>,
    pub build_recipe: Option<BuildRecipe>,
    pub package_layout: Option<PackageLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidanceSet {
    pub tree_sha256: String,
    pub items: Vec<GuidanceItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidanceItem {
    pub id: String,
    pub title: String,
    pub source_path: String,
    pub scenarios: Vec<GuidanceScenario>,
    pub asset_types: Vec<String>,
    pub always: bool,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidanceScenario {
    Planner,
    AssetCodegen,
    CustomCodeCodegen,
    AssetGroupCodegen,
}

impl GuidanceScenario {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::AssetCodegen => "asset_codegen",
            Self::CustomCodeCodegen => "custom_code_codegen",
            Self::AssetGroupCodegen => "asset_group_codegen",
        }
    }
}

impl LoadedGamePack {
    #[must_use]
    pub fn resource_spec(&self, raw_asset_type: &str) -> Option<&AssetResourceSpec> {
        let normalized = normalize_asset_type(raw_asset_type);
        self.resource_specs
            .iter()
            .find(|spec| normalize_asset_type(&spec.id) == normalized)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTemplate {
    pub placeholder: String,
    pub tree_sha256: String,
    pub files: Vec<ProjectTemplateFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTemplateFile {
    /// Normalized, forward-slash path relative to the declared template root.
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonManifestContract {
    /// Project-relative path. `{mod_id}` is the only placeholder.
    pub relative_path: String,
    pub expected: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRecipe {
    pub local_properties: Vec<BuildLocalProperty>,
    pub steps: Vec<BuildStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildLocalProperty {
    pub input_key: String,
    pub property: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildStep {
    pub id: String,
    pub runner: BuildRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildRunner {
    DotnetPublish,
}

impl BuildRunner {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DotnetPublish => "dotnet_publish",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLayout {
    /// Source-root-relative files. `{mod_id}` is the only placeholder.
    pub required_files: Vec<String>,
}

fn normalize_asset_type(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetResourceSpec {
    pub id: String,
    pub localization: LocalizationResourceSpec,
    pub images: Vec<ImageResourceSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizationResourceSpec {
    pub table: String,
    /// Path relative to the generated mod resource root. `{locale}` is the only placeholder.
    pub relative_path: String,
    pub locales: Vec<String>,
    pub required_suffixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResourceSpec {
    pub role: ResourceImageRole,
    /// Path relative to the generated mod resource root. `{slug}` is the only placeholder.
    pub relative_path: String,
    pub transform: ResourceImageTransform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceImageRole {
    Normal,
    Outline,
    Big,
}

impl ResourceImageRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Outline => "outline",
            Self::Big => "big",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceImageTransform {
    Preserve,
    Cover {
        width: u32,
        height: u32,
    },
    Outline {
        width: u32,
        height: u32,
        radius: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationRule {
    ForbiddenCallInMethod {
        id: String,
        method_name: String,
        call_path: Vec<String>,
        message: String,
    },
}

impl ValidationRule {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::ForbiddenCallInMethod { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruthSource {
    pub id: String,
    pub kind: TruthSourceKind,
    pub indexer: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruthSourceKind {
    LocalFile {
        input_key: String,
    },
    GitHubReleaseAsset {
        repository: String,
        pinned_release: String,
        asset: String,
        sha256: String,
    },
}
