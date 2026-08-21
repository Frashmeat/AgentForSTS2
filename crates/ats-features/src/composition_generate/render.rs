use std::collections::BTreeMap;

use ats_game_context::{
    BehaviorAdapterError, BehaviorAdapterRegistry, BehaviorAdapterRegistryError,
    BehaviorItemContext, BehaviorItemReference, BehaviorProposal, BehaviorRenderContext,
    BehaviorResourceBinding, CapabilityValue, LoadedGamePack, RenderedFile, RenderedItemBundle,
    VerifiedContributionSet,
};
use ats_kernel::FailureCode;
use ats_runtime::{ProjectFileWrite, ProjectWriteError, RunFailure};
use ats_workspace::{
    ItemFieldValue, ItemReferenceBinding, LocalizationStatus, ResourceRepository,
    StoredItemDefinition,
};
use thiserror::Error;

use crate::FeatureSpec;
use crate::composition::ResolvedItemGraph;
use crate::resource_prepare::{
    ResourcePrepareError, ResourcePrepareFeature, ResourceSpecs, expand_target_template,
};

pub(super) struct RenderItemRequest<'a> {
    pub graph: &'a ResolvedItemGraph,
    pub definition: &'a StoredItemDefinition,
    pub mod_id: &'a str,
    pub proposal: &'a BehaviorProposal,
}

pub(super) fn render_item<R: ResourceRepository + ?Sized>(
    registry: &BehaviorAdapterRegistry,
    pack: &LoadedGamePack,
    resource_contributions: &VerifiedContributionSet,
    resources: &R,
    request: RenderItemRequest<'_>,
) -> Result<(RenderedItemBundle, Vec<ProjectFileWrite>), BehaviorRenderError> {
    let RenderItemRequest {
        graph,
        definition,
        mod_id,
        proposal,
    } = request;
    if graph.game_pack_id != *pack.id()
        || graph.game_pack_sha256 != *pack.content_sha256()
        || definition.definition.item_id != proposal.item_id
        || definition.definition.item_type != proposal.item_type
        || definition.definition_hash != proposal.definition_hash
    {
        return Err(BehaviorRenderError::InvalidContext);
    }
    let (context, mut resource_files) = render_context(
        pack,
        resource_contributions,
        resources,
        graph,
        definition,
        mod_id,
    )?;
    let adapter_bundle = registry.render(
        pack.behavior_adapter(),
        pack.capability_catalog(),
        &context,
        proposal,
    )?;
    let mut files = adapter_bundle.files;
    files.append(&mut resource_files);
    let bundle =
        RenderedItemBundle::new(proposal, files).map_err(BehaviorAdapterRegistryError::Adapter)?;
    let writes = bundle
        .files
        .iter()
        .map(|file| ProjectFileWrite::new(file.relative_path.clone(), file.bytes.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    ats_runtime::validate_project_writes(&writes)?;
    Ok((bundle, writes))
}

fn render_context<R: ResourceRepository + ?Sized>(
    pack: &LoadedGamePack,
    resource_contributions: &VerifiedContributionSet,
    resources: &R,
    graph: &ResolvedItemGraph,
    definition: &StoredItemDefinition,
    mod_id: &str,
) -> Result<(BehaviorRenderContext, Vec<RenderedFile>), BehaviorRenderError> {
    let item = &definition.definition;
    let canonical_fields = item
        .canonical_fields
        .iter()
        .map(|(id, value)| (id.clone(), capability_value(value)))
        .collect();
    let localizations = item
        .localizations
        .iter()
        .map(|(locale, localization)| {
            if localization.status != LocalizationStatus::Confirmed {
                return Err(BehaviorRenderError::InvalidContext);
            }
            Ok((locale.clone(), localization.fields.clone()))
        })
        .collect::<Result<_, _>>()?;
    let references = item
        .reference_bindings
        .iter()
        .map(|(slot, bindings)| {
            let values = bindings
                .iter()
                .map(|binding| reference(graph, binding))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((slot.clone(), values))
        })
        .collect::<Result<_, BehaviorRenderError>>()?;
    let (resource_bindings, resource_files) =
        render_resources(pack, resource_contributions, resources, definition, mod_id)?;
    Ok((
        BehaviorRenderContext {
            mod_id: mod_id.to_owned(),
            game_pack_id: pack.id().clone(),
            game_pack_sha256: pack.content_sha256().clone(),
            truth_snapshot_id: graph.truth_snapshot_id.clone(),
            catalog: pack.capability_catalog_identity().clone(),
            adapter: pack.behavior_adapter().clone(),
            item: BehaviorItemContext {
                item_id: item.item_id.clone(),
                item_type: item.item_type.clone(),
                definition_hash: definition.definition_hash.clone(),
                canonical_fields,
                localizations,
                references,
                resources: resource_bindings,
            },
        },
        resource_files,
    ))
}

fn capability_value(value: &ItemFieldValue) -> CapabilityValue {
    match value {
        ItemFieldValue::Text(value) => CapabilityValue::Text(value.clone()),
        ItemFieldValue::Integer(value) => CapabilityValue::Integer(*value),
        ItemFieldValue::Boolean(value) => CapabilityValue::Boolean(*value),
        ItemFieldValue::Choice(value) => CapabilityValue::Choice(value.clone()),
        ItemFieldValue::StringList(value) => CapabilityValue::TextList(value.clone()),
    }
}

fn reference(
    graph: &ResolvedItemGraph,
    binding: &ItemReferenceBinding,
) -> Result<BehaviorItemReference, BehaviorRenderError> {
    match binding {
        ItemReferenceBinding::Pinned {
            item_id,
            definition_hash,
            quantity,
        } => {
            let target = graph
                .nodes
                .iter()
                .find(|target| {
                    &target.definition.item_id == item_id
                        && &target.definition_hash == definition_hash
                })
                .ok_or(BehaviorRenderError::InvalidReference)?;
            Ok(BehaviorItemReference {
                item_id: item_id.clone(),
                item_type: target.definition.item_type.clone(),
                definition_hash: definition_hash.clone(),
                quantity: *quantity,
            })
        }
        ItemReferenceBinding::Identity {
            item_id,
            expected_item_type,
        } => {
            let target = graph
                .nodes
                .iter()
                .find(|target| {
                    &target.definition.item_id == item_id
                        && &target.definition.item_type == expected_item_type
                })
                .ok_or(BehaviorRenderError::InvalidReference)?;
            Ok(BehaviorItemReference {
                item_id: item_id.clone(),
                item_type: expected_item_type.clone(),
                definition_hash: target.definition_hash.clone(),
                quantity: 1,
            })
        }
    }
}

fn render_resources<R: ResourceRepository + ?Sized>(
    pack: &LoadedGamePack,
    resource_contributions: &VerifiedContributionSet,
    resources: &R,
    definition: &StoredItemDefinition,
    mod_id: &str,
) -> Result<
    (
        BTreeMap<ats_kernel::ResourceId, BehaviorResourceBinding>,
        Vec<RenderedFile>,
    ),
    BehaviorRenderError,
> {
    if resource_contributions.feature_id() != &ResourcePrepareFeature::id()
        || resource_contributions.game_pack_id() != pack.id()
        || resource_contributions.game_pack_sha256() != pack.content_sha256()
    {
        return Err(BehaviorRenderError::InvalidContext);
    }
    let specs: ResourceSpecs = resource_contributions.decode(
        &ats_kernel::ContributionId::parse("resource.prepare.specs")?,
    )?;
    specs.validate()?;
    let mut bindings = BTreeMap::new();
    let mut files = Vec::new();
    for (logical_role, selected) in &definition.definition.resource_bindings {
        let asset = resources
            .load(&selected.resource_id)
            .map_err(|_| BehaviorRenderError::ResourceStorage)?;
        if asset.resource_id() != &selected.resource_id
            || asset.logical_role() != logical_role.as_str()
            || asset.selected_version() != Some(&selected.selected_version)
        {
            return Err(BehaviorRenderError::InvalidResource);
        }
        let selected_version = asset
            .selected()
            .ok_or(BehaviorRenderError::InvalidResource)?;
        specs.validate_blob(asset.logical_role(), &selected_version.blob)?;
        let spec = specs.require_role(asset.logical_role(), &selected_version.blob.media_type)?;
        let target = spec
            .target_path
            .as_deref()
            .ok_or(BehaviorRenderError::InvalidResource)?;
        let relative_path =
            expand_target_template(target, mod_id, definition.definition.item_id.as_str())?;
        let bytes = resources
            .read_selected_bytes(&selected.resource_id, &selected.selected_version)
            .map_err(|_| BehaviorRenderError::ResourceStorage)?;
        files.push(RenderedFile::new(
            logical_role.to_string(),
            relative_path.clone(),
            bytes,
        )?);
        let entry = bindings
            .entry(selected.resource_id.clone())
            .or_insert_with(|| BehaviorResourceBinding {
                resource_id: selected.resource_id.clone(),
                selected_version: selected.selected_version.clone(),
                published_paths: BTreeMap::new(),
            });
        if entry.selected_version != selected.selected_version
            || entry
                .published_paths
                .insert(logical_role.to_string(), relative_path)
                .is_some()
        {
            return Err(BehaviorRenderError::InvalidResource);
        }
    }
    Ok((bindings, files))
}

#[derive(Debug, Error)]
pub enum BehaviorRenderError {
    #[error("behavior render context is invalid")]
    InvalidContext,
    #[error("behavior render reference is invalid")]
    InvalidReference,
    #[error("behavior render resource is invalid")]
    InvalidResource,
    #[error("behavior render resource storage failed")]
    ResourceStorage,
    #[error(transparent)]
    Adapter(#[from] BehaviorAdapterRegistryError),
    #[error("behavior deterministic output is invalid")]
    AdapterOutput(#[from] BehaviorAdapterError),
    #[error(transparent)]
    Contribution(#[from] ats_game_context::ContributionResolverError),
    #[error(transparent)]
    Resource(#[from] ResourcePrepareError),
    #[error(transparent)]
    Contract(#[from] ats_kernel::ContractValueError),
    #[error(transparent)]
    Write(#[from] ProjectWriteError),
}

impl BehaviorRenderError {
    pub(super) fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidContext | Self::InvalidReference => (
                "composition.execution.invalid",
                "composition.render.context",
            ),
            Self::InvalidResource => ("resource.source_invalid", "composition.render.resource"),
            Self::ResourceStorage => ("resource.storage_failed", "composition.render.resource"),
            Self::Adapter(BehaviorAdapterRegistryError::Unavailable) => {
                ("game.adapter_unavailable", "composition.render.adapter")
            }
            Self::Adapter(BehaviorAdapterRegistryError::InvalidIr) => {
                ("behavior.ir_invalid", "composition.render.adapter")
            }
            Self::Adapter(BehaviorAdapterRegistryError::Adapter(
                BehaviorAdapterError::UnsupportedCapability,
            )) => ("game.adapter_unsupported", "composition.render.adapter"),
            Self::Adapter(_) | Self::AdapterOutput(_) => {
                ("game.adapter_invalid", "composition.render.adapter")
            }
            Self::Contribution(_) | Self::Resource(_) => {
                ("pack.contribution_invalid", "composition.render.pack")
            }
            Self::Contract(_) => (
                "composition.execution.invalid",
                "composition.render.contract",
            ),
            Self::Write(ProjectWriteError::InvalidWrite | ProjectWriteError::DuplicatePath) => {
                ("composition.staging.invalid", "composition.render.output")
            }
            Self::Write(ProjectWriteError::Io { .. }) => {
                ("composition.staging.failed", "composition.render.output")
            }
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            None,
        )
        .expect("built-in Run failure is valid")
    }
}
