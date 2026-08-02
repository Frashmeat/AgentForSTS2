use std::path::Path;

use ats_game_context::{
    ContributionResolverError, LoadedGamePack, ProjectTemplateError, VerifiedContributionSet,
    built_in_project_template,
};
use ats_kernel::{ContributionId, FeatureId, SchemaId, SchemaRef, SchemaVersion};
use ats_workspace::{ProjectError, ProjectFolder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;

pub struct ProjectCreateFeature;

impl FeatureSpec for ProjectCreateFeature {
    type Request = ProjectCreateRequest;
    type Result = ProjectCreateResult;
    type ArtifactExtension = ProjectCreateArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("project.create").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.project-create-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.project-create-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.project-create-artifact-extension")
    }
}

impl ProjectCreateFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: template_slot(),
            schema: schema("pack.project-template"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCreateRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCreateResult {
    pub name: String,
    pub csharp_name: String,
    pub game_pack_id: ats_kernel::GamePackId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCreateArtifactExtension {
    pub template_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TemplateContribution {
    template_id: String,
}

pub struct ProjectCreateService;

impl ProjectCreateService {
    pub fn execute(
        &self,
        parent: &Path,
        request: &ProjectCreateRequest,
        pack: &LoadedGamePack,
        contributions: &VerifiedContributionSet,
    ) -> Result<(ProjectFolder, ProjectCreateResult), ProjectCreateError> {
        if request.name.trim().is_empty() || request.name.chars().count() > 128 {
            return Err(ProjectCreateError::InvalidInput);
        }
        if contributions.feature_id() != &ProjectCreateFeature::id()
            || contributions.game_pack_id() != pack.id()
            || contributions.game_pack_sha256() != pack.content_sha256()
        {
            return Err(ProjectCreateError::ContextIdentityMismatch);
        }
        let contribution: TemplateContribution = contributions.decode(&template_slot())?;
        let template = built_in_project_template(pack, &contribution.template_id)?;
        let folder = ProjectFolder::create(parent, &request.name, pack.id(), &template)?;
        let result = ProjectCreateResult {
            name: folder.meta().name.clone(),
            csharp_name: folder.meta().csharp_name.clone(),
            game_pack_id: pack.id().clone(),
        };
        Ok((folder, result))
    }
}

#[derive(Debug, Error)]
pub enum ProjectCreateError {
    #[error("project create input is invalid")]
    InvalidInput,
    #[error("project create context identities do not match")]
    ContextIdentityMismatch,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Template(#[from] ProjectTemplateError),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

fn template_slot() -> ContributionId {
    ContributionId::parse("project.create.template").expect("built-in contribution ID is valid")
}

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}
