use std::collections::BTreeSet;
use std::path::Path;

use ats_game_context::{ContributionResolverError, LoadedGamePack, VerifiedContributionSet};
use ats_kernel::{ContributionId, FeatureId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest};
use ats_runtime::{
    ArtifactFileInput, ArtifactPublishRequest, ArtifactPublisher, CancellationToken, PackageEntry,
    PackageError, PackagePrepareRequest, PackageReport, PackageWriter, PayloadError,
    RunLifecycleError, RunRecord, RunStatus, RunTransition, VersionedPayload,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;

pub struct ProjectPackageFeature;

impl FeatureSpec for ProjectPackageFeature {
    type Request = ProjectPackageRequest;
    type Result = ProjectPackageResult;
    type ArtifactExtension = ProjectPackageArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("project.package").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.project-package-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.project-package-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.project-package-artifact-extension")
    }
}

impl ProjectPackageFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: package_slot(),
            schema: schema("pack.package-layout"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPackageRequest {
    pub artifact_id: String,
    pub mod_id: String,
    pub source_relative_root: String,
    pub output_relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression_level: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPackageResult {
    pub artifact_manifest_ref: String,
    pub manifest_sha256: Sha256Digest,
    pub output_relative_path: String,
    pub report: PackageReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPackageArtifactExtension {
    pub output_relative_path: String,
    pub report: PackageReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageLayout {
    required_files: Vec<String>,
}

impl PackageLayout {
    fn entries(&self, mod_id: &str) -> Result<Vec<PackageEntry>, ProjectPackageError> {
        if self.required_files.is_empty() || self.required_files.len() > 512 {
            return Err(ProjectPackageError::InvalidLayout);
        }
        let mut paths = BTreeSet::new();
        let mut entries = Vec::with_capacity(self.required_files.len());
        for template in &self.required_files {
            let path = expand_template(template, mod_id)?;
            if !paths.insert(path.clone()) {
                return Err(ProjectPackageError::InvalidLayout);
            }
            entries.push(PackageEntry::new(path.clone(), path)?);
        }
        Ok(entries)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageContext {
    game_pack_id: ats_kernel::GamePackId,
    game_pack_sha256: Sha256Digest,
    contribution_slot: ContributionId,
}

pub struct ProjectPackageContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub project_root: &'a Path,
}

pub struct ProjectPackageService;

impl ProjectPackageService {
    pub fn execute<W, A>(
        &self,
        writer: &W,
        artifacts: &A,
        run: &mut RunRecord,
        request: ProjectPackageRequest,
        context: ProjectPackageContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ProjectPackageResult, ProjectPackageError>
    where
        W: PackageWriter + ?Sized,
        A: ArtifactPublisher + ?Sized,
    {
        validate_run(run, &request)?;
        validate_context(&context)?;
        validate_request(&request)?;
        check_cancelled(cancellation)?;
        let layout: PackageLayout = context.contributions.decode(&package_slot())?;
        let entries = layout.entries(&request.mod_id)?;
        let pending = writer.prepare(
            PackagePrepareRequest {
                project_root: context.project_root.to_path_buf(),
                source_relative_root: request.source_relative_root.clone(),
                output_relative_path: request.output_relative_path.clone(),
                run_id: run.id().clone(),
                entries,
                compression_level: request.compression_level,
            },
            cancellation,
        )?;
        if let Err(error) = check_cancelled(cancellation) {
            pending.rollback()?;
            return Err(error);
        }
        let extension = ProjectPackageArtifactExtension {
            output_relative_path: pending.output_relative_path().to_owned(),
            report: pending.report().clone(),
        };
        let published = match artifacts.publish(ArtifactPublishRequest {
            artifact_id: request.artifact_id.clone(),
            artifact_kind: "package".into(),
            feature_id: ProjectPackageFeature::id(),
            producing_run_id: run.id().clone(),
            contexts: vec![VersionedPayload::from_typed(
                schema("artifact.package-context"),
                &PackageContext {
                    game_pack_id: context.pack.id().clone(),
                    game_pack_sha256: context.pack.content_sha256().clone(),
                    contribution_slot: package_slot(),
                },
            )?],
            provenance: Vec::new(),
            feature_extension: VersionedPayload::from_typed(
                ProjectPackageFeature::artifact_extension_schema(),
                &extension,
            )?,
            files: vec![ArtifactFileInput {
                role: "package.zip".into(),
                source_path: pending.output_path().to_path_buf(),
                published_relative_path: Some(pending.output_relative_path().to_owned()),
            }],
        }) {
            Ok(published) => published,
            Err(_) => {
                pending.rollback()?;
                return Err(ProjectPackageError::ArtifactPublication);
            }
        };
        if let Err(error) = check_cancelled(cancellation) {
            cleanup(artifacts, &request.artifact_id, run.id())?;
            pending.rollback()?;
            return Err(error);
        }
        let result = ProjectPackageResult {
            artifact_manifest_ref: published.artifact_manifest_ref,
            manifest_sha256: published.manifest_sha256,
            output_relative_path: extension.output_relative_path,
            report: extension.report,
        };
        let payload =
            VersionedPayload::from_typed(ProjectPackageFeature::result_schema(), &result)?;
        if run
            .apply_transition(RunTransition::Succeed { result: payload }, Utc::now())
            .is_err()
        {
            cleanup(artifacts, &request.artifact_id, run.id())?;
            pending.rollback()?;
            return Err(ProjectPackageError::RunTransition);
        }
        pending.commit()?;
        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum ProjectPackageError {
    #[error("project package input is invalid")]
    InvalidInput,
    #[error("project package Run does not match its typed request")]
    InvalidRun,
    #[error("project package context identities do not match")]
    ContextIdentityMismatch,
    #[error("project package Pack layout is invalid")]
    InvalidLayout,
    #[error("project package Artifact publication failed")]
    ArtifactPublication,
    #[error("project package Artifact cleanup failed")]
    ArtifactCleanup,
    #[error("project package Run transition failed")]
    RunTransition,
    #[error("project package was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Lifecycle(#[from] RunLifecycleError),
}

fn validate_run(
    run: &RunRecord,
    request: &ProjectPackageRequest,
) -> Result<(), ProjectPackageError> {
    if run.feature_id() != &ProjectPackageFeature::id() || run.status() != RunStatus::Running {
        return Err(ProjectPackageError::InvalidRun);
    }
    let persisted = run
        .request()
        .decode::<ProjectPackageRequest>(&ProjectPackageFeature::request_schema())
        .map_err(|_| ProjectPackageError::InvalidRun)?;
    if &persisted != request {
        return Err(ProjectPackageError::InvalidRun);
    }
    Ok(())
}

fn validate_context(context: &ProjectPackageContext<'_>) -> Result<(), ProjectPackageError> {
    if context.contributions.feature_id() != &ProjectPackageFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(ProjectPackageError::ContextIdentityMismatch);
    }
    Ok(())
}

fn validate_request(request: &ProjectPackageRequest) -> Result<(), ProjectPackageError> {
    if !valid_segment(&request.artifact_id)
        || !valid_segment(&request.mod_id)
        || request.source_relative_root.starts_with(".ats/")
        || request.output_relative_path.starts_with(".ats/")
        || ats_runtime::normalize_relative_path(Path::new(&request.source_relative_root)).is_err()
        || ats_runtime::normalize_relative_path(Path::new(&request.output_relative_path)).is_err()
        || request
            .compression_level
            .is_some_and(|level| !(0..=9).contains(&level))
    {
        return Err(ProjectPackageError::InvalidInput);
    }
    Ok(())
}

fn expand_template(template: &str, mod_id: &str) -> Result<String, ProjectPackageError> {
    if template.is_empty()
        || template.len() > 512
        || template.contains('\\')
        || !valid_segment(mod_id)
    {
        return Err(ProjectPackageError::InvalidLayout);
    }
    let path = template.replace("{mod_id}", mod_id);
    if path.contains('{')
        || path.contains('}')
        || ats_runtime::normalize_relative_path(Path::new(&path)).is_err()
        || path.starts_with(".ats/")
    {
        return Err(ProjectPackageError::InvalidLayout);
    }
    Ok(path)
}

fn cleanup<A: ArtifactPublisher + ?Sized>(
    artifacts: &A,
    artifact_id: &str,
    run_id: &ats_runtime::RunId,
) -> Result<(), ProjectPackageError> {
    artifacts
        .remove_published_run(artifact_id, run_id)
        .map_err(|_| ProjectPackageError::ArtifactCleanup)
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), ProjectPackageError> {
    if cancellation.is_cancelled() {
        Err(ProjectPackageError::Cancelled)
    } else {
        Ok(())
    }
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn package_slot() -> ContributionId {
    ContributionId::parse("project.package.layout").expect("built-in contribution ID is valid")
}

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}

#[cfg(test)]
mod tests {
    use ats_game_context::{ContributionResolver, GamePackLoader};
    use ats_kernel::PrimitiveId;
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn synthetic_pack_uses_the_same_build_and_package_contracts() {
        let value = serde_json::json!({
            "schemaVersion":3,
            "id":"fixture-game",
            "displayName":"Fixture Game",
            "itemTypes":[{
                "id":"fixture_item",
                "displayNames":{"eng":"Fixture item"},
                "evidenceQueries":[{"symbols":["Fixture.Symbol"],"terms":[]}]
            }],
            "contributions":[
                {
                    "slotId":"project.build.recipe",
                    "featureId":"project.build",
                    "schema":{"id":"pack.build-recipe","version":1},
                    "requiredPrimitives":["process.fixture-build"],
                    "payload":{"steps":[{"id":"assemble","primitive":"process.fixture-build"}]}
                },
                {
                    "slotId":"project.package.layout",
                    "featureId":"project.package",
                    "schema":{"id":"pack.package-layout","version":1},
                    "payload":{"requiredFiles":["runtime/core.bin","mods/{mod_id}.bundle"]}
                }
            ]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let digest = Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes))).unwrap();
        let pack = GamePackLoader::load(&bytes, &digest).unwrap();
        ContributionResolver::new([PrimitiveId::parse("process.fixture-build").unwrap()])
            .resolve(
                &pack,
                &crate::project_build::ProjectBuildFeature::id(),
                &[crate::project_build::ProjectBuildFeature::contribution_requirement()],
            )
            .unwrap();
        let contributions = ContributionResolver::new(Vec::<PrimitiveId>::new())
            .resolve(
                &pack,
                &ProjectPackageFeature::id(),
                &[ProjectPackageFeature::contribution_requirement()],
            )
            .unwrap();
        let layout: PackageLayout = contributions.decode(&package_slot()).unwrap();
        let entries = layout.entries("FixtureMod").unwrap();
        assert_eq!(entries[1].archive_path(), "mods/FixtureMod.bundle");
    }
}
