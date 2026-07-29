use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Deserialize;

use super::asset_compile::{AssetCompileValidator, CompileValidation};
use super::code_generate::{
    extract_first_code_block, sanitize_entity_name, validate_generated_code_skein,
};
use super::common::{ProgressEvent, ProgressSink, emit_cancelled_mid_stream, is_cancelled};
use crate::codegen::{AssetCodegenRequest, AssetKind, asset_localization_key_segment};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::domain::{JobId, JobRepository};

const MAX_MODEL_ATTEMPTS: u32 = 2;
static ASSET_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) struct AssetBundleGeneration {
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    compile_validator: Arc<dyn AssetCompileValidator>,
    sink: Arc<dyn ProgressSink>,
}

impl AssetBundleGeneration {
    pub(crate) fn new(
        repo: Arc<dyn JobRepository>,
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
        job_id: &JobId,
        prompt: String,
        evidence_record: &str,
        request: &AssetCodegenRequest,
        artifacts_dir: &Path,
        runtime_image_source: Option<&Path>,
    ) -> Result<WrittenAssetBundle, AssetBundleError> {
        validate_project_scope(&request.project_root, artifacts_dir)
            .map_err(AssetBundleError::Write)?;
        if request.asset_name.trim().is_empty() {
            return Err(AssetBundleError::ModelOutput(
                "asset_name must not be empty".into(),
            ));
        }
        let asset_kind = AssetKind::parse(&request.asset_type).ok_or_else(|| {
            AssetBundleError::ModelOutput(format!(
                "unsupported asset_type for structured generation: {}",
                request.asset_type
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
        let mut final_raw = String::new();
        let mut parsed_bundle = None;
        let mut last_model_error = String::new();

        for attempt in 1..=MAX_MODEL_ATTEMPTS {
            let completion = self.collect_once(job_id, prompt.clone()).await?;
            usage_in = usage_in.saturating_add(completion.usage_in);
            usage_out = usage_out.saturating_add(completion.usage_out);
            final_model = completion.model;
            final_raw = completion.raw;

            let parsed = parse_and_validate_bundle(&final_raw, asset_kind, &expected_key);
            match parsed {
                Ok(bundle) => {
                    parsed_bundle = Some(bundle);
                    break;
                }
                Err(err) => {
                    last_model_error = err;
                    if attempt < MAX_MODEL_ATTEMPTS {
                        self.sink
                            .emit(ProgressEvent {
                                job_id: job_id.clone(),
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
        if is_cancelled(&self.repo, job_id).await {
            emit_cancelled_mid_stream(&self.sink, job_id).await;
            return Err(AssetBundleError::Cancelled);
        }

        let _write_guard = ASSET_WRITE_LOCK.lock().await;
        let mut artifact = write_artifacts(
            artifacts_dir,
            &entity_name,
            &bundle.csharp,
            &final_raw,
            evidence_record,
        )
        .await
        .map_err(AssetBundleError::Write)?;
        let planned = plan_project_writes(
            request,
            asset_kind,
            &mod_id,
            &entity_name,
            &bundle,
            runtime_image_source,
        )
        .await
        .map_err(AssetBundleError::Write)?;
        artifact.cs_path = planned.cs_path;
        artifact.localization_paths = planned.localization_paths;
        artifact.runtime_image_paths = planned.runtime_image_paths;
        let transaction = FileTransaction::apply(planned.writes)
            .await
            .map_err(AssetBundleError::Write)?;

        self.sink
            .emit(ProgressEvent {
                job_id: job_id.clone(),
                stage: "compile-gate".into(),
                percent: None,
                message: Some("validating generated asset with dotnet build".into()),
                delta: None,
            })
            .await;
        let compile = self
            .compile_validator
            .validate(&request.project_root, job_id)
            .await;
        if is_cancelled(&self.repo, job_id).await {
            let rollback = transaction.rollback().await;
            if let Err(err) = rollback {
                return Err(AssetBundleError::Write(format!(
                    "cancelled and failed to roll back generated files: {err}"
                )));
            }
            emit_cancelled_mid_stream(&self.sink, job_id).await;
            return Err(AssetBundleError::Cancelled);
        }
        let compile = match compile {
            Ok(report) => report,
            Err(err) => {
                let rollback = transaction.rollback().await;
                return Err(AssetBundleError::Compile(match rollback {
                    Ok(()) => err,
                    Err(rollback_err) => {
                        format!("{err}\nrollback generated files failed: {rollback_err}")
                    }
                }));
            }
        };
        transaction.commit();

        Ok(WrittenAssetBundle {
            model: final_model,
            entity_name,
            cs_path: artifact.cs_path,
            artifact_cs_path: artifact.artifact_cs_path,
            raw_path: artifact.raw_path,
            evidence_path: artifact.evidence_path,
            localization_paths: artifact.localization_paths,
            runtime_image_paths: artifact.runtime_image_paths,
            extracted_chars: bundle.csharp.len(),
            raw_chars: final_raw.len(),
            usage_in,
            usage_out,
            compile,
        })
    }

    async fn collect_once(
        &self,
        job_id: &JobId,
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
            if tick.is_multiple_of(5) && is_cancelled(&self.repo, job_id).await {
                emit_cancelled_mid_stream(&self.sink, job_id).await;
                return Err(AssetBundleError::Cancelled);
            }
            match item {
                Ok(StreamEvent::Start { model: value }) => model = value,
                Ok(StreamEvent::Delta { text }) => {
                    raw.push_str(&text);
                    self.sink
                        .emit(ProgressEvent {
                            job_id: job_id.clone(),
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
    pub artifact_cs_path: PathBuf,
    pub raw_path: PathBuf,
    pub evidence_path: PathBuf,
    pub localization_paths: Vec<PathBuf>,
    pub runtime_image_paths: Vec<PathBuf>,
    pub extracted_chars: usize,
    pub raw_chars: usize,
    pub usage_in: u32,
    pub usage_out: u32,
    pub compile: CompileValidation,
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
    localization: ModelLocalization,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelLocalization {
    eng: BTreeMap<String, String>,
    zhs: BTreeMap<String, String>,
}

fn parse_and_validate_bundle(
    raw: &str,
    asset_kind: AssetKind,
    expected_key: &str,
) -> Result<ModelAssetBundle, String> {
    if raw.trim().is_empty() {
        return Err("model produced an empty response".into());
    }
    let candidate = extract_first_code_block(raw).unwrap_or_else(|| raw.to_string());
    let bundle: ModelAssetBundle = serde_json::from_str(candidate.trim())
        .map_err(|err| format!("parse strict asset bundle JSON: {err}"))?;
    validate_generated_code_skein(&bundle.csharp)?;
    validate_localization(asset_kind, expected_key, &bundle.localization)?;
    Ok(bundle)
}

fn validate_localization(
    asset_kind: AssetKind,
    expected_key: &str,
    localization: &ModelLocalization,
) -> Result<(), String> {
    if localization.eng.is_empty() || localization.zhs.is_empty() {
        return Err("asset bundle must include non-empty eng and zhs localization".into());
    }
    let eng_keys: BTreeSet<&str> = localization.eng.keys().map(String::as_str).collect();
    let zhs_keys: BTreeSet<&str> = localization.zhs.keys().map(String::as_str).collect();
    if eng_keys != zhs_keys {
        return Err("eng and zhs localization keys must match exactly".into());
    }
    let prefix = format!("{expected_key}.");
    for key in &eng_keys {
        if !key.starts_with(&prefix) {
            return Err(format!("localization key must start with {prefix}: {key}"));
        }
    }
    for suffix in asset_kind.required_localization_suffixes() {
        let key = format!("{expected_key}.{suffix}");
        if !localization.eng.contains_key(&key) {
            return Err(format!("missing required localization key: {key}"));
        }
    }
    for (locale, entries) in [("eng", &localization.eng), ("zhs", &localization.zhs)] {
        if let Some((key, _)) = entries.iter().find(|(_, value)| value.trim().is_empty()) {
            return Err(format!("{locale} localization value is empty: {key}"));
        }
    }
    Ok(())
}

struct ArtifactPaths {
    cs_path: PathBuf,
    artifact_cs_path: PathBuf,
    raw_path: PathBuf,
    evidence_path: PathBuf,
    localization_paths: Vec<PathBuf>,
    runtime_image_paths: Vec<PathBuf>,
}

async fn write_artifacts(
    artifacts_dir: &Path,
    entity_name: &str,
    csharp: &str,
    raw: &str,
    evidence_record: &str,
) -> Result<ArtifactPaths, String> {
    let target_dir = artifacts_dir.join(entity_name);
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|err| format!("create asset artifact dir: {err}"))?;
    let artifact_cs_path = target_dir.join(format!("{entity_name}.cs"));
    let raw_path = target_dir.join("raw.md");
    let evidence_path = target_dir.join("evidence.md");
    crate::fs_atomic::write_atomic(&artifact_cs_path, csharp.as_bytes())
        .await
        .map_err(|err| format!("write artifact C#: {err}"))?;
    crate::fs_atomic::write_atomic(&raw_path, raw.as_bytes())
        .await
        .map_err(|err| format!("write raw model output: {err}"))?;
    crate::fs_atomic::write_atomic(&evidence_path, evidence_record.as_bytes())
        .await
        .map_err(|err| format!("write evidence record: {err}"))?;
    Ok(ArtifactPaths {
        cs_path: PathBuf::new(),
        artifact_cs_path,
        raw_path,
        evidence_path,
        localization_paths: Vec::new(),
        runtime_image_paths: Vec::new(),
    })
}

struct PlannedProjectBundle {
    writes: Vec<PlannedWrite>,
    cs_path: PathBuf,
    localization_paths: Vec<PathBuf>,
    runtime_image_paths: Vec<PathBuf>,
}

async fn plan_project_writes(
    request: &AssetCodegenRequest,
    asset_kind: AssetKind,
    mod_id: &str,
    entity_name: &str,
    bundle: &ModelAssetBundle,
    runtime_image_source: Option<&Path>,
) -> Result<PlannedProjectBundle, String> {
    let cs_path = request
        .project_root
        .join("Generated")
        .join(format!("{entity_name}.cs"));
    let table = asset_kind.localization_table();
    let mut writes = vec![PlannedWrite {
        path: cs_path.clone(),
        bytes: bundle.csharp.as_bytes().to_vec(),
    }];
    let mut localization_paths = Vec::new();
    for (locale, additions) in [
        ("eng", &bundle.localization.eng),
        ("zhs", &bundle.localization.zhs),
    ] {
        let path = request
            .project_root
            .join(mod_id)
            .join("localization")
            .join(locale)
            .join(format!("{table}.json"));
        let bytes = merge_localization(&path, additions).await?;
        localization_paths.push(path.clone());
        writes.push(PlannedWrite { path, bytes });
    }
    let mut runtime_image_paths = Vec::new();
    if let Some(source) = runtime_image_source {
        let bytes = tokio::fs::read(source)
            .await
            .map_err(|err| format!("read generated runtime image {}: {err}", source.display()))?;
        let expected_paths = runtime_image_paths_for(request)?;
        if request.image_paths != expected_paths {
            return Err("generated image destinations do not match the asset path contract".into());
        }
        for path in expected_paths {
            runtime_image_paths.push(path.clone());
            writes.push(PlannedWrite {
                path,
                bytes: bytes.clone(),
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
) -> Result<Vec<PathBuf>, String> {
    let kind = AssetKind::parse(&request.asset_type).ok_or_else(|| {
        format!(
            "unsupported asset_type for image delivery: {}",
            request.asset_type
        )
    })?;
    let mod_id = load_mod_id(&request.project_root)?;
    let slug = asset_localization_key_segment(&request.asset_name).to_ascii_lowercase();
    if slug.is_empty() {
        return Err(
            "asset_name must contain at least one ASCII letter or digit for image delivery".into(),
        );
    }
    let root = request.project_root.join(mod_id).join("images");
    let relative: Vec<PathBuf> = match kind {
        AssetKind::Card | AssetKind::CardFullscreen => vec![
            PathBuf::from("card_portraits").join(format!("{slug}.png")),
            PathBuf::from("card_portraits")
                .join("big")
                .join(format!("{slug}.png")),
        ],
        AssetKind::Relic => vec![
            PathBuf::from("relics").join(format!("{slug}.png")),
            PathBuf::from("relics").join(format!("{slug}_outline.png")),
            PathBuf::from("relics")
                .join("big")
                .join(format!("{slug}.png")),
        ],
        AssetKind::Power => vec![
            PathBuf::from("powers").join(format!("{slug}.png")),
            PathBuf::from("powers")
                .join("big")
                .join(format!("{slug}.png")),
        ],
        AssetKind::Character => {
            vec![PathBuf::from("characters").join(format!("{slug}.png"))]
        }
    };
    Ok(relative.into_iter().map(|path| root.join(path)).collect())
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

struct FileTransaction {
    snapshots: Vec<FileSnapshot>,
}

impl FileTransaction {
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

    async fn rollback(mut self) -> Result<(), String> {
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

    fn commit(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let raw = relic_bundle();
        assert!(
            parse_and_validate_bundle(&raw, AssetKind::Relic, "DEMOMOD-ENERGY_SEED_RELIC").is_ok()
        );
        let fenced = format!("```json\n{raw}\n```");
        assert!(
            parse_and_validate_bundle(&fenced, AssetKind::Relic, "DEMOMOD-ENERGY_SEED_RELIC")
                .is_ok()
        );
    }

    #[test]
    fn rejects_missing_language_and_wrong_prefix() {
        let missing_zhs =
            r#"{"csharp":"public class X {}","localization":{"eng":{"X.title":"x"},"zhs":{}}}"#;
        assert!(parse_and_validate_bundle(missing_zhs, AssetKind::Relic, "DEMOMOD-X").is_err());

        let wrong_prefix = relic_bundle().replace("DEMOMOD-", "OTHER-");
        assert!(
            parse_and_validate_bundle(&wrong_prefix, AssetKind::Relic, "DEMOMOD-ENERGY_SEED_RELIC")
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
    async fn file_transaction_rolls_back_old_and_new_files() {
        let td = tempfile::TempDir::new().unwrap();
        let old = td.path().join("old.json");
        let new = td.path().join("new.cs");
        tokio::fs::write(&old, b"old").await.unwrap();
        let transaction = FileTransaction::apply(vec![
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
}
