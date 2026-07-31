use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Deserialize;

use super::asset_compile::AssetCompileValidator;
use super::code_generate::{
    extract_first_code_block, sanitize_entity_name, validate_generated_code_skein,
};
use super::common::{ProgressEvent, ProgressSink, emit_cancelled_mid_stream, is_cancelled};
use crate::codegen::{AssetCodegenRequest, asset_localization_key_segment};
use crate::game_pack::{
    AssetResourceSpec, LoadedGamePack, ResourceImageRole, ResourceImageTransform, ValidationRule,
};
use crate::image_proc::{
    ImageQualitySpec, ImageVariantRole, ImageVariantSpec, ImageVariantTransform,
    analyze_png_quality, derive_png_variants,
};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::domain::{RunId, RunRepository};

const MAX_MODEL_ATTEMPTS: u32 = 2;
static ASSET_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) struct AssetBundleGeneration {
    repo: Arc<dyn RunRepository>,
    llm: Arc<dyn LlmClient>,
    compile_validator: Arc<dyn AssetCompileValidator>,
    sink: Arc<dyn ProgressSink>,
}

impl AssetBundleGeneration {
    pub(crate) fn new(
        repo: Arc<dyn RunRepository>,
        llm: Arc<dyn LlmClient>,
        compile_validator: Arc<dyn AssetCompileValidator>,
        sink: Arc<dyn ProgressSink>,
    ) -> Self {
        Self {
            repo,
            llm,
            compile_validator,
            sink,
        }
    }

    pub(crate) async fn generate(
        &self,
        run_id: &RunId,
        prompt: String,
        pack: &LoadedGamePack,
        request: &AssetCodegenRequest,
        runtime_image_source: Option<&Path>,
    ) -> Result<WrittenAssetBundle, AssetBundleError> {
        if request.asset_name.trim().is_empty() {
            return Err(AssetBundleError::ModelOutput(
                "asset_name must not be empty".into(),
            ));
        }
        let resource_spec = pack.resource_spec(&request.asset_type).ok_or_else(|| {
            AssetBundleError::ModelOutput(format!(
                "game pack `{}` does not declare structured asset type `{}`",
                pack.id, request.asset_type
            ))
        })?;
        let entity_name = sanitize_entity_name(&request.asset_name);
        let mod_id = load_mod_id(&request.project_root).map_err(AssetBundleError::Write)?;
        let expected_key = format!(
            "{}-{}",
            mod_id.to_ascii_uppercase(),
            asset_localization_key_segment(&request.asset_name)
        );

        let mut usage_in = 0_u32;
        let mut usage_out = 0_u32;
        let mut final_model = String::new();
        let mut parsed_bundle = None;
        let mut last_model_error = String::new();

        for attempt in 1..=MAX_MODEL_ATTEMPTS {
            let completion = self.collect_once(run_id, prompt.clone()).await?;
            usage_in = usage_in.saturating_add(completion.usage_in);
            usage_out = usage_out.saturating_add(completion.usage_out);
            let parsed = parse_and_validate_bundle(
                &completion.raw,
                &pack.validation_rules,
                resource_spec,
                &expected_key,
            );
            match parsed {
                Ok(bundle) => {
                    final_model = completion.model;
                    parsed_bundle = Some(bundle);
                    break;
                }
                Err(err) => {
                    last_model_error = err;
                    if attempt < MAX_MODEL_ATTEMPTS {
                        self.sink
                            .emit(ProgressEvent {
                                run_id: run_id.clone(),
                                stage: "model-output-retry".into(),
                                percent: None,
                                message: Some(format!(
                                    "invalid model output; retrying ({attempt}/{MAX_MODEL_ATTEMPTS})"
                                )),
                                delta: None,
                            })
                            .await;
                    }
                }
            }
        }

        let bundle = parsed_bundle.ok_or_else(|| {
            AssetBundleError::ModelOutput(format!(
                "model output remained invalid after {MAX_MODEL_ATTEMPTS} attempts: {last_model_error}"
            ))
        })?;
        if is_cancelled(&self.repo, run_id).await {
            emit_cancelled_mid_stream(&self.sink, run_id).await;
            return Err(AssetBundleError::Cancelled);
        }

        let _write_guard = ASSET_WRITE_LOCK.lock().await;
        let planned = plan_project_writes(
            request,
            resource_spec,
            &mod_id,
            &entity_name,
            &bundle,
            runtime_image_source,
        )
        .await
        .map_err(AssetBundleError::Write)?;
        let cs_path = planned.cs_path.clone();
        let localization_paths = planned.localization_paths.clone();
        let runtime_image_paths = planned.runtime_image_paths.clone();
        let transaction = ProjectFileTransaction::apply(planned.writes)
            .await
            .map_err(AssetBundleError::Write)?;

        self.sink
            .emit(ProgressEvent {
                run_id: run_id.clone(),
                stage: "compile-gate".into(),
                percent: None,
                message: Some("validating generated asset with dotnet build".into()),
                delta: None,
            })
            .await;
        let compile = self
            .compile_validator
            .validate(&request.project_root, run_id)
            .await;
        if is_cancelled(&self.repo, run_id).await {
            let rollback = transaction.rollback().await;
            if let Err(err) = rollback {
                return Err(AssetBundleError::Write(format!(
                    "cancelled and failed to roll back generated files: {err}"
                )));
            }
            emit_cancelled_mid_stream(&self.sink, run_id).await;
            return Err(AssetBundleError::Cancelled);
        }
        if let Err(err) = compile {
            let rollback = transaction.rollback().await;
            return Err(AssetBundleError::Compile(match rollback {
                Ok(()) => err,
                Err(rollback_err) => {
                    format!("{err}\nrollback generated files failed: {rollback_err}")
                }
            }));
        }
        Ok(WrittenAssetBundle {
            model: final_model,
            entity_name,
            cs_path,
            localization_paths,
            runtime_image_paths,
            usage_in,
            usage_out,
            transaction: Some(transaction),
        })
    }

    async fn collect_once(
        &self,
        run_id: &RunId,
        prompt: String,
    ) -> Result<RawCompletion, AssetBundleError> {
        let request = CompletionRequest {
            messages: vec![Message {
                role: MessageRole::User,
                content: prompt,
            }],
            system_prompt: None,
            max_tokens: 8192,
            temperature: None,
            model: None,
        };
        let mut stream = self
            .llm
            .stream(request)
            .await
            .map_err(|err| AssetBundleError::Stream(err.to_string()))?;
        let mut raw = String::new();
        let mut model = String::new();
        let mut usage_in = 0;
        let mut usage_out = 0;
        let mut tick = 0_u32;
        while let Some(item) = stream.next().await {
            tick = tick.wrapping_add(1);
            if tick.is_multiple_of(5) && is_cancelled(&self.repo, run_id).await {
                emit_cancelled_mid_stream(&self.sink, run_id).await;
                return Err(AssetBundleError::Cancelled);
            }
            match item {
                Ok(StreamEvent::Start { model: value }) => model = value,
                Ok(StreamEvent::Delta { text }) => {
                    raw.push_str(&text);
                    self.sink
                        .emit(ProgressEvent {
                            run_id: run_id.clone(),
                            stage: "stream-delta".into(),
                            percent: None,
                            message: None,
                            delta: Some(text),
                        })
                        .await;
                }
                Ok(StreamEvent::End { usage, .. }) => {
                    usage_in = usage.input_tokens;
                    usage_out = usage.output_tokens;
                }
                Err(err) => return Err(AssetBundleError::Stream(err.to_string())),
            }
        }
        Ok(RawCompletion {
            raw,
            model,
            usage_in,
            usage_out,
        })
    }
}

pub(crate) struct WrittenAssetBundle {
    pub model: String,
    pub entity_name: String,
    pub cs_path: PathBuf,
    pub localization_paths: Vec<PathBuf>,
    pub runtime_image_paths: Vec<PathBuf>,
    pub usage_in: u32,
    pub usage_out: u32,
    transaction: Option<ProjectFileTransaction>,
}

impl WrittenAssetBundle {
    pub(crate) fn commit_writes(&mut self) {
        if let Some(transaction) = self.transaction.take() {
            transaction.commit();
        }
    }

    pub(crate) async fn rollback_writes(&mut self) -> Result<(), String> {
        match self.transaction.take() {
            Some(transaction) => transaction.rollback().await,
            None => Ok(()),
        }
    }
}

pub(crate) enum AssetBundleError {
    Stream(String),
    ModelOutput(String),
    Write(String),
    Compile(String),
    Cancelled,
}

struct RawCompletion {
    raw: String,
    model: String,
    usage_in: u32,
    usage_out: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelAssetBundle {
    csharp: String,
    localization: BTreeMap<String, BTreeMap<String, String>>,
}

fn parse_and_validate_bundle(
    raw: &str,
    validation_rules: &[ValidationRule],
    resource_spec: &AssetResourceSpec,
    expected_key: &str,
) -> Result<ModelAssetBundle, String> {
    if raw.trim().is_empty() {
        return Err("model produced an empty response".into());
    }
    let candidate = extract_first_code_block(raw).unwrap_or_else(|| raw.to_string());
    let bundle: ModelAssetBundle = serde_json::from_str(candidate.trim())
        .map_err(|err| format!("parse strict asset bundle JSON: {err}"))?;
    validate_generated_code_skein(&bundle.csharp, validation_rules)?;
    validate_localization(resource_spec, expected_key, &bundle.localization)?;
    Ok(bundle)
}

fn validate_localization(
    resource_spec: &AssetResourceSpec,
    expected_key: &str,
    localization: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), String> {
    let expected_locales = resource_spec
        .localization
        .locales
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let actual_locales = localization
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_locales != expected_locales {
        return Err(format!(
            "localization locales must match resource spec exactly: expected {}, found {}",
            expected_locales.into_iter().collect::<Vec<_>>().join(", "),
            actual_locales.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    let Some(reference) = resource_spec
        .localization
        .locales
        .first()
        .and_then(|locale| localization.get(locale))
    else {
        return Err("resource spec must declare at least one localization locale".into());
    };
    let expected_keys = reference
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if expected_keys.is_empty() {
        return Err("asset bundle localization maps must not be empty".into());
    }
    for locale in &resource_spec.localization.locales {
        let entries = &localization[locale];
        let keys = entries.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if keys != expected_keys {
            return Err("localization keys must match exactly across all locales".into());
        }
    }
    let prefix = format!("{expected_key}.");
    for key in &expected_keys {
        if !key.starts_with(&prefix) {
            return Err(format!("localization key must start with {prefix}: {key}"));
        }
    }
    for suffix in &resource_spec.localization.required_suffixes {
        let key = format!("{expected_key}.{suffix}");
        if !reference.contains_key(&key) {
            return Err(format!("missing required localization key: {key}"));
        }
    }
    for (locale, entries) in localization {
        if let Some((key, _)) = entries.iter().find(|(_, value)| value.trim().is_empty()) {
            return Err(format!("{locale} localization value is empty: {key}"));
        }
    }
    Ok(())
}

struct PlannedProjectBundle {
    writes: Vec<PlannedWrite>,
    cs_path: PathBuf,
    localization_paths: Vec<PathBuf>,
    runtime_image_paths: Vec<PathBuf>,
}

async fn plan_project_writes(
    request: &AssetCodegenRequest,
    resource_spec: &AssetResourceSpec,
    mod_id: &str,
    entity_name: &str,
    bundle: &ModelAssetBundle,
    runtime_image_source: Option<&Path>,
) -> Result<PlannedProjectBundle, String> {
    let cs_path = request
        .project_root
        .join("Generated")
        .join(format!("{entity_name}.cs"));
    let mut writes = vec![PlannedWrite {
        path: cs_path.clone(),
        bytes: bundle.csharp.as_bytes().to_vec(),
    }];
    let mut localization_paths = Vec::new();
    for locale in &resource_spec.localization.locales {
        let additions = &bundle.localization[locale];
        let relative_path = resource_spec
            .localization
            .relative_path
            .replace("{locale}", locale);
        let path = request.project_root.join(mod_id).join(relative_path);
        let bytes = merge_localization(&path, additions).await?;
        localization_paths.push(path.clone());
        writes.push(PlannedWrite { path, bytes });
    }
    let mut runtime_image_paths = Vec::new();
    if let Some(source) = runtime_image_source {
        let bytes = tokio::fs::read(source)
            .await
            .map_err(|err| format!("read generated runtime image {}: {err}", source.display()))?;
        let targets = runtime_image_targets_for(request, resource_spec)?;
        let expected_paths = targets
            .iter()
            .map(|target| target.path.clone())
            .collect::<Vec<_>>();
        if request.image_paths != expected_paths {
            return Err("generated image destinations do not match the asset path contract".into());
        }
        let specs = targets.iter().map(|target| target.spec).collect::<Vec<_>>();
        let variants = derive_png_variants(&bytes, &specs)
            .map_err(|err| format!("derive runtime image roles: {err}"))?;
        validate_role_derivation(&variants, &specs)?;
        for (target, variant) in targets.into_iter().zip(variants) {
            if target.spec.role != variant.role {
                return Err("derived runtime image role order does not match resource spec".into());
            }
            runtime_image_paths.push(target.path.clone());
            writes.push(PlannedWrite {
                path: target.path,
                bytes: variant.bytes,
            });
        }
    }
    Ok(PlannedProjectBundle {
        writes,
        cs_path,
        localization_paths,
        runtime_image_paths,
    })
}

pub(crate) fn runtime_image_paths_for(
    request: &AssetCodegenRequest,
    pack: &LoadedGamePack,
) -> Result<Vec<PathBuf>, String> {
    let resource_spec = pack.resource_spec(&request.asset_type).ok_or_else(|| {
        format!(
            "game pack `{}` does not declare structured asset type `{}`",
            pack.id, request.asset_type
        )
    })?;
    Ok(runtime_image_targets_for(request, resource_spec)?
        .into_iter()
        .map(|target| target.path)
        .collect())
}

#[derive(Debug)]
struct RuntimeImageTarget {
    path: PathBuf,
    spec: ImageVariantSpec,
}

fn runtime_image_targets_for(
    request: &AssetCodegenRequest,
    resource_spec: &AssetResourceSpec,
) -> Result<Vec<RuntimeImageTarget>, String> {
    let mod_id = load_mod_id(&request.project_root)?;
    let slug = asset_localization_key_segment(&request.asset_name).to_ascii_lowercase();
    if slug.is_empty() {
        return Err(
            "asset_name must contain at least one ASCII letter or digit for image delivery".into(),
        );
    }
    let root = request.project_root.join(mod_id);
    Ok(resource_spec
        .images
        .iter()
        .map(|image| RuntimeImageTarget {
            path: root.join(image.relative_path.replace("{slug}", &slug)),
            spec: image_variant_spec(image.role, image.transform),
        })
        .collect())
}

fn image_variant_spec(
    role: ResourceImageRole,
    transform: ResourceImageTransform,
) -> ImageVariantSpec {
    let role = match role {
        ResourceImageRole::Normal => ImageVariantRole::Normal,
        ResourceImageRole::Outline => ImageVariantRole::Outline,
        ResourceImageRole::Big => ImageVariantRole::Big,
    };
    match transform {
        ResourceImageTransform::Preserve => ImageVariantSpec {
            role,
            width: 0,
            height: 0,
            transform: ImageVariantTransform::Preserve,
        },
        ResourceImageTransform::Cover { width, height } => ImageVariantSpec {
            role,
            width,
            height,
            transform: ImageVariantTransform::Cover,
        },
        ResourceImageTransform::Outline {
            width,
            height,
            radius,
        } => ImageVariantSpec {
            role,
            width,
            height,
            transform: ImageVariantTransform::Outline { radius },
        },
    }
}

fn validate_role_derivation(
    variants: &[crate::image_proc::DerivedImageVariant],
    specs: &[ImageVariantSpec],
) -> Result<(), String> {
    let requires_distinct_roles = specs
        .iter()
        .any(|spec| spec.transform != ImageVariantTransform::Preserve);
    if !requires_distinct_roles {
        return Ok(());
    }
    for left in 0..variants.len() {
        for right in left + 1..variants.len() {
            if variants[left].bytes == variants[right].bytes {
                return Err(format!(
                    "resource role derivation produced identical {:?} and {:?} PNG bytes",
                    variants[left].role, variants[right].role
                ));
            }
        }
    }
    for (variant, spec) in variants.iter().zip(specs) {
        if spec.transform == ImageVariantTransform::Preserve {
            continue;
        }
        if (variant.width, variant.height) != (spec.width, spec.height) {
            return Err(format!(
                "derived {:?} image is {}x{}, expected {}x{}",
                variant.role, variant.width, variant.height, spec.width, spec.height
            ));
        }
        let report = analyze_png_quality(&variant.bytes, ImageQualitySpec::default())
            .map_err(|err| format!("validate derived {:?} image: {err}", variant.role))?;
        match variant.role {
            ImageVariantRole::Normal | ImageVariantRole::Big if !report.accepted => {
                return Err(format!(
                    "derived {:?} image failed quality rules: {}",
                    variant.role,
                    report.rejection_summary()
                ));
            }
            ImageVariantRole::Outline
                if report.foreground_pixels == 0 || report.transparent_pixels == 0 =>
            {
                return Err(
                    "derived Outline image must contain both outline pixels and transparency"
                        .into(),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

async fn merge_localization(
    path: &Path,
    additions: &BTreeMap<String, String>,
) -> Result<Vec<u8>, String> {
    let mut merged: BTreeMap<String, String> = if path.is_file() {
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|err| format!("read existing localization {}: {err}", path.display()))?;
        serde_json::from_str(&text).map_err(|err| {
            format!(
                "existing localization must be a flat string map ({}): {err}",
                path.display()
            )
        })?
    } else {
        BTreeMap::new()
    };
    merged.extend(additions.clone());
    let mut text = serde_json::to_string_pretty(&merged)
        .map_err(|err| format!("serialize merged localization: {err}"))?;
    text.push('\n');
    Ok(text.into_bytes())
}

fn load_mod_id(project_root: &Path) -> Result<String, String> {
    let path = project_root.join("project.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("read project identity {}: {err}", path.display()))?;
    let meta: crate::project::ProjectMeta = serde_json::from_str(&text)
        .map_err(|err| format!("parse project identity {}: {err}", path.display()))?;
    let mod_id = meta.csharp_name.trim();
    if mod_id.is_empty() {
        return Err("project identity has an empty csharp_name".into());
    }
    if !mod_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(format!("project csharp_name is not a safe ModId: {mod_id}"));
    }
    Ok(mod_id.to_string())
}

pub(crate) fn validate_project_scope(
    project_root: &Path,
    artifacts_dir: &Path,
) -> Result<(), String> {
    let artifact_root = artifacts_dir.parent().ok_or_else(|| {
        format!(
            "artifacts directory has no project parent: {}",
            artifacts_dir.display()
        )
    })?;
    let requested = std::fs::canonicalize(project_root).map_err(|err| {
        format!(
            "resolve requested project root {}: {err}",
            project_root.display()
        )
    })?;
    let active = std::fs::canonicalize(artifact_root).map_err(|err| {
        format!(
            "resolve active project root {}: {err}",
            artifact_root.display()
        )
    })?;
    if requested != active {
        return Err(format!(
            "asset project_root must match the active project (requested {}, active {})",
            requested.display(),
            active.display()
        ));
    }
    Ok(())
}

struct PlannedWrite {
    path: PathBuf,
    bytes: Vec<u8>,
}

struct FileSnapshot {
    path: PathBuf,
    previous: Option<Vec<u8>>,
}

pub(crate) struct ProjectFileTransaction {
    snapshots: Vec<FileSnapshot>,
}

impl ProjectFileTransaction {
    async fn apply(writes: Vec<PlannedWrite>) -> Result<Self, String> {
        let mut transaction = Self {
            snapshots: Vec::new(),
        };
        for write in writes {
            let previous = match tokio::fs::read(&write.path).await {
                Ok(bytes) => Some(bytes),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err) => {
                    let _ = transaction.rollback_in_place().await;
                    return Err(format!("snapshot {}: {err}", write.path.display()));
                }
            };
            if let Some(parent) = write.path.parent()
                && let Err(err) = tokio::fs::create_dir_all(parent).await
            {
                let _ = transaction.rollback_in_place().await;
                return Err(format!("create output dir {}: {err}", parent.display()));
            }
            transaction.snapshots.push(FileSnapshot {
                path: write.path.clone(),
                previous,
            });
            if let Err(err) = crate::fs_atomic::write_atomic(&write.path, &write.bytes).await {
                let rollback = transaction.rollback_in_place().await;
                return Err(match rollback {
                    Ok(()) => format!("write generated file {}: {err}", write.path.display()),
                    Err(rollback_err) => format!(
                        "write generated file {}: {err}; rollback failed: {rollback_err}",
                        write.path.display()
                    ),
                });
            }
        }
        Ok(transaction)
    }

    pub(crate) async fn write_one(path: PathBuf, bytes: Vec<u8>) -> Result<Self, String> {
        Self::apply(vec![PlannedWrite { path, bytes }]).await
    }

    pub(crate) async fn rollback(mut self) -> Result<(), String> {
        self.rollback_in_place().await
    }

    async fn rollback_in_place(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        for snapshot in self.snapshots.iter().rev() {
            let result = match &snapshot.previous {
                Some(bytes) => crate::fs_atomic::write_atomic(&snapshot.path, bytes).await,
                None => match tokio::fs::remove_file(&snapshot.path).await {
                    Ok(()) => Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(err) => Err(err),
                },
            };
            if let Err(err) = result {
                failures.push(format!("{}: {err}", snapshot.path.display()));
            }
        }
        self.snapshots.clear();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    pub(crate) fn commit(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sts2_pack() -> LoadedGamePack {
        crate::game_pack::GamePackRegistry::built_in()
            .unwrap()
            .require("sts2")
            .unwrap()
            .clone()
    }

    fn prepare_project_identity(root: &Path) {
        std::fs::write(
            root.join("project.json"),
            serde_json::json!({
                "name": "Demo Mod",
                "csharp_name": "DemoMod",
                "game_id": "sts2",
                "scaffolded": true,
                "generated_files": [],
                "build_output_dir": null
            })
            .to_string(),
        )
        .unwrap();
    }

    fn relic_bundle() -> String {
        serde_json::json!({
            "csharp": "using BaseLib.Abstracts;\npublic sealed class EnergySeedRelic {}",
            "localization": {
                "eng": {
                    "DEMOMOD-ENERGY_SEED_RELIC.title": "Energy Seed",
                    "DEMOMOD-ENERGY_SEED_RELIC.description": "Gain energy.",
                    "DEMOMOD-ENERGY_SEED_RELIC.flavor": "It hums."
                },
                "zhs": {
                    "DEMOMOD-ENERGY_SEED_RELIC.title": "能量种子",
                    "DEMOMOD-ENERGY_SEED_RELIC.description": "获得能量。",
                    "DEMOMOD-ENERGY_SEED_RELIC.flavor": "它在嗡鸣。"
                }
            }
        })
        .to_string()
    }

    #[test]
    fn parses_good_and_fenced_bundle() {
        let pack = sts2_pack();
        let relic = pack.resource_spec("relic").unwrap();
        let raw = relic_bundle();
        assert!(parse_and_validate_bundle(&raw, &[], relic, "DEMOMOD-ENERGY_SEED_RELIC").is_ok());
        let fenced = format!("```json\n{raw}\n```");
        assert!(
            parse_and_validate_bundle(&fenced, &[], relic, "DEMOMOD-ENERGY_SEED_RELIC").is_ok()
        );
    }

    #[test]
    fn rejects_missing_language_and_wrong_prefix() {
        let pack = sts2_pack();
        let relic = pack.resource_spec("relic").unwrap();
        let missing_zhs =
            r#"{"csharp":"public class X {}","localization":{"eng":{"X.title":"x"},"zhs":{}}}"#;
        assert!(parse_and_validate_bundle(missing_zhs, &[], relic, "DEMOMOD-X").is_err());

        let wrong_prefix = relic_bundle().replace("DEMOMOD-", "OTHER-");
        assert!(
            parse_and_validate_bundle(&wrong_prefix, &[], relic, "DEMOMOD-ENERGY_SEED_RELIC",)
                .is_err()
        );
    }

    #[tokio::test]
    async fn localization_merge_preserves_existing_entries() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("relics.json");
        tokio::fs::write(&path, r#"{"EXISTING.title":"Existing"}"#)
            .await
            .unwrap();
        let additions = BTreeMap::from([("NEW.title".into(), "New".into())]);
        let bytes = merge_localization(&path, &additions).await.unwrap();
        let merged: BTreeMap<String, String> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(merged.get("EXISTING.title").unwrap(), "Existing");
        assert_eq!(merged.get("NEW.title").unwrap(), "New");
    }

    #[tokio::test]
    async fn sts2_resource_specs_preserve_all_output_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        prepare_project_identity(temp.path());
        let pack = sts2_pack();
        let cases = [
            (
                "card",
                "cards.json",
                vec![
                    "images/card_portraits/energy_seed.png",
                    "images/card_portraits/big/energy_seed.png",
                ],
            ),
            (
                "card_fullscreen",
                "cards.json",
                vec![
                    "images/card_portraits/energy_seed.png",
                    "images/card_portraits/big/energy_seed.png",
                ],
            ),
            (
                "relic",
                "relics.json",
                vec![
                    "images/relics/energy_seed.png",
                    "images/relics/energy_seed_outline.png",
                    "images/relics/big/energy_seed.png",
                ],
            ),
            (
                "power",
                "powers.json",
                vec![
                    "images/powers/energy_seed.png",
                    "images/powers/big/energy_seed.png",
                ],
            ),
            (
                "character",
                "characters.json",
                vec!["images/characters/energy_seed.png"],
            ),
        ];
        for (asset_type, table, expected_images) in cases {
            let spec = pack.resource_spec(asset_type).unwrap();
            let request = AssetCodegenRequest {
                asset_type: asset_type.into(),
                asset_name: "EnergySeed".into(),
                project_root: temp.path().to_path_buf(),
                ..Default::default()
            };
            let expected_key = "DEMOMOD-ENERGY_SEED";
            let entries = spec
                .localization
                .required_suffixes
                .iter()
                .map(|suffix| (format!("{expected_key}.{suffix}"), "value".into()))
                .collect::<BTreeMap<_, _>>();
            let bundle = ModelAssetBundle {
                csharp: "public class EnergySeed {}".into(),
                localization: spec
                    .localization
                    .locales
                    .iter()
                    .map(|locale| (locale.clone(), entries.clone()))
                    .collect(),
            };
            let planned =
                plan_project_writes(&request, spec, "DemoMod", "EnergySeed", &bundle, None)
                    .await
                    .unwrap();
            assert_eq!(
                planned.localization_paths,
                ["eng", "zhs"].map(|locale| temp
                    .path()
                    .join("DemoMod/localization")
                    .join(locale)
                    .join(table))
            );
            assert_eq!(
                runtime_image_paths_for(&request, &pack).unwrap(),
                expected_images
                    .into_iter()
                    .map(|relative| temp.path().join("DemoMod").join(relative))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn file_transaction_rolls_back_old_and_new_files() {
        let td = tempfile::TempDir::new().unwrap();
        let old = td.path().join("old.json");
        let new = td.path().join("new.cs");
        tokio::fs::write(&old, b"old").await.unwrap();
        let transaction = ProjectFileTransaction::apply(vec![
            PlannedWrite {
                path: old.clone(),
                bytes: b"changed".to_vec(),
            },
            PlannedWrite {
                path: new.clone(),
                bytes: b"new".to_vec(),
            },
        ])
        .await
        .unwrap();
        transaction.rollback().await.unwrap();
        assert_eq!(tokio::fs::read(old).await.unwrap(), b"old");
        assert!(!new.exists());
    }

    #[test]
    fn project_scope_rejects_a_different_project_root() {
        let td = tempfile::TempDir::new().unwrap();
        let active = td.path().join("active");
        let other = td.path().join("other");
        std::fs::create_dir_all(active.join("artifacts")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let err = validate_project_scope(&other, &active.join("artifacts")).unwrap_err();
        assert!(err.contains("must match the active project"));
    }

    #[test]
    fn relic_resource_spec_rejects_identical_role_bytes() {
        let pack = sts2_pack();
        let relic = pack.resource_spec("relic").unwrap();
        let specs = relic
            .images
            .iter()
            .map(|image| image_variant_spec(image.role, image.transform))
            .collect::<Vec<_>>();
        assert_eq!(specs[0].role, ImageVariantRole::Normal);
        assert_eq!((specs[0].width, specs[0].height), (128, 128));
        assert_eq!(specs[1].role, ImageVariantRole::Outline);
        assert_eq!(specs[2].role, ImageVariantRole::Big);
        assert_eq!((specs[2].width, specs[2].height), (1024, 1024));
        let variants = [
            crate::image_proc::DerivedImageVariant {
                role: ImageVariantRole::Normal,
                bytes: vec![1, 2, 3],
                width: 128,
                height: 128,
            },
            crate::image_proc::DerivedImageVariant {
                role: ImageVariantRole::Outline,
                bytes: vec![1, 2, 3],
                width: 128,
                height: 128,
            },
            crate::image_proc::DerivedImageVariant {
                role: ImageVariantRole::Big,
                bytes: vec![4, 5, 6],
                width: 1024,
                height: 1024,
            },
        ];

        let error = validate_role_derivation(&variants, &specs).unwrap_err();

        assert!(error.contains("identical Normal and Outline"));
    }
}
