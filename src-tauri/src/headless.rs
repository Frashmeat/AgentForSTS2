use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

use ats_kernel::{BuildInfo, ItemId};
use ats_runtime::VersionedPayload;
use ats_workspace::{CompositionDraftNode, ItemDefinition};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bootstrap;
use crate::commands::failure::{CommandFailure, CommandResult};
use crate::commands::{health, project, settings, stage2};
use crate::composition::Stage2Composition;
use crate::project_session::ActiveProject;
use crate::{AppConfig, AppPaths};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_REQUEST_ID_CHARS: usize = 128;

pub(crate) fn run() -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return 1,
    };
    runtime.block_on(run_stdio())
}

async fn run_stdio() -> i32 {
    let desktop = match bootstrap::load() {
        Ok(runtime) => HeadlessRuntime {
            active: ActiveProject::new(),
            paths: AppPaths::new(runtime.app_data),
            config: runtime.config,
            composition: runtime.composition,
        },
        Err(failure) => {
            let response = HeadlessResponse::failure(None, failure);
            let mut stdout = io::stdout().lock();
            let _ = write_response(&mut stdout, &response);
            return 1;
        }
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_protocol(stdin.lock(), stdout.lock(), desktop).await
}

struct HeadlessRuntime {
    active: ActiveProject,
    paths: AppPaths,
    config: Arc<AppConfig>,
    composition: Arc<Stage2Composition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeadlessRequest {
    schema_version: u32,
    request_id: String,
    command: HeadlessCommandEnvelope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeadlessCommandEnvelope {
    name: HeadlessCommandName,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HeadlessCommandName {
    Health,
    GetSettings,
    SaveSettings,
    CreateProject,
    OpenProject,
    CloseProject,
    CurrentProject,
    GetTruthStatus,
    ImportTruth,
    GetFeatureCatalog,
    SubmitFeature,
    ListRuns,
    GetRun,
    ListExecutionGraphs,
    GetExecutionGraph,
    ResumeExecutionGraph,
    ListItemDefinitions,
    GetItemDefinition,
    SaveItemDefinition,
    ListCompositionDrafts,
    GetCompositionDraft,
    UpdateCompositionDraft,
    ConfirmCompositionDraft,
    ListResourceAssets,
    SelectResource,
    SubmitCompositionItemFeedback,
    Shutdown,
}

#[derive(Debug)]
enum HeadlessCommand {
    Health,
    GetSettings,
    SaveSettings(SaveSettingsInput),
    CreateProject(CreateProjectInput),
    OpenProject(PathInput),
    CloseProject,
    CurrentProject,
    GetTruthStatus,
    ImportTruth,
    GetFeatureCatalog,
    SubmitFeature(SubmitFeatureInput),
    ListRuns,
    GetRun(RunIdInput),
    ListExecutionGraphs,
    GetExecutionGraph(ExecutionGraphIdInput),
    ResumeExecutionGraph(ResumeExecutionGraphInput),
    ListItemDefinitions,
    GetItemDefinition(GetItemDefinitionInput),
    SaveItemDefinition(SaveItemDefinitionInput),
    ListCompositionDrafts,
    GetCompositionDraft(DraftIdInput),
    UpdateCompositionDraft(UpdateCompositionDraftInput),
    ConfirmCompositionDraft(ConfirmCompositionDraftInput),
    ListResourceAssets,
    SelectResource(SelectResourceInput),
    SubmitCompositionItemFeedback(SubmitCompositionItemFeedbackInput),
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveSettingsInput {
    patch: settings::SettingsPatch,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateProjectInput {
    parent_dir: String,
    project_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PathInput {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitFeatureInput {
    feature_id: ats_kernel::FeatureId,
    request: VersionedPayload,
    #[serde(default)]
    source_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunIdInput {
    run_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutionGraphIdInput {
    execution_graph_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResumeExecutionGraphInput {
    execution_graph_id: String,
    expected_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GetItemDefinitionInput {
    item_id: String,
    #[serde(default)]
    definition_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveItemDefinitionInput {
    definition: ItemDefinition,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DraftIdInput {
    draft_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateCompositionDraftInput {
    draft_id: String,
    expected_revision: u64,
    nodes: BTreeMap<ItemId, CompositionDraftNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmCompositionDraftInput {
    draft_id: String,
    expected_revision: u64,
    selected_item_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectResourceInput {
    resource_id: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitCompositionItemFeedbackInput {
    request: stage2::AdjustCompositionItemRequest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeadlessResponse {
    schema_version: u32,
    request_id: Option<String>,
    build: BuildInfo,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<CommandFailure>,
}

impl HeadlessResponse {
    fn success(request_id: String, result: Value) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            request_id: Some(request_id),
            build: crate::build_identity::current(),
            ok: true,
            result: Some(result),
            failure: None,
        }
    }

    fn failure(request_id: Option<String>, failure: CommandFailure) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            request_id,
            build: crate::build_identity::current(),
            ok: false,
            result: None,
            failure: Some(failure),
        }
    }
}

async fn run_protocol<R, W>(mut input: R, mut output: W, runtime: HeadlessRuntime) -> i32
where
    R: BufRead,
    W: Write,
{
    let mut line = Vec::new();
    loop {
        let response = match read_bounded_line(&mut input, &mut line) {
            Ok(BoundedLine::Eof) => return shutdown(&runtime, 0).await,
            Err(_) => return shutdown(&runtime, 1).await,
            Ok(BoundedLine::TooLarge) => HeadlessResponse::failure(
                None,
                CommandFailure::invalid_input("headless.request.size"),
            ),
            Ok(BoundedLine::Line) => match std::str::from_utf8(&line) {
                Ok(value) if value.trim().is_empty() => continue,
                Ok(value) => handle_line(&runtime, value).await,
                Err(_) => HeadlessResponse::failure(
                    None,
                    CommandFailure::invalid_input("headless.request.decode"),
                ),
            },
        };
        let should_stop = response.ok
            && response
                .result
                .as_ref()
                .is_some_and(|value| value["shutdown"] == true);
        if write_response(&mut output, &response).is_err() {
            return shutdown(&runtime, 1).await;
        }
        if should_stop {
            return 0;
        }
    }
}

enum BoundedLine {
    Eof,
    Line,
    TooLarge,
}

fn read_bounded_line<R: BufRead>(input: &mut R, output: &mut Vec<u8>) -> io::Result<BoundedLine> {
    output.clear();
    let mut read_any = false;
    let mut too_large = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return if !read_any {
                Ok(BoundedLine::Eof)
            } else if too_large {
                Ok(BoundedLine::TooLarge)
            } else {
                Ok(BoundedLine::Line)
            };
        }
        read_any = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if !too_large {
            if output.len().saturating_add(consumed) <= MAX_REQUEST_BYTES {
                output.extend_from_slice(&available[..consumed]);
            } else {
                too_large = true;
                output.clear();
            }
        }
        input.consume(consumed);
        if newline.is_some() {
            return if too_large {
                Ok(BoundedLine::TooLarge)
            } else {
                Ok(BoundedLine::Line)
            };
        }
    }
}

async fn handle_line(runtime: &HeadlessRuntime, line: &str) -> HeadlessResponse {
    let request = match serde_json::from_str::<HeadlessRequest>(line) {
        Ok(request) => request,
        Err(_) => {
            return HeadlessResponse::failure(
                request_id_from_invalid_line(line),
                CommandFailure::invalid_input("headless.request.decode"),
            );
        }
    };
    let request_id = request.request_id;
    if request.schema_version != PROTOCOL_VERSION || !valid_request_id(&request_id) {
        return HeadlessResponse::failure(
            valid_request_id(&request_id).then_some(request_id),
            CommandFailure::invalid_input("headless.request.contract"),
        );
    }
    let command = match decode_command(request.command) {
        Ok(command) => command,
        Err(failure) => return HeadlessResponse::failure(Some(request_id), failure),
    };
    match dispatch(runtime, command).await {
        Ok(result) => HeadlessResponse::success(request_id, result),
        Err(failure) => HeadlessResponse::failure(Some(request_id), failure),
    }
}

fn decode_command(envelope: HeadlessCommandEnvelope) -> CommandResult<HeadlessCommand> {
    let input = envelope.input;
    match envelope.name {
        HeadlessCommandName::Health => no_input(input, HeadlessCommand::Health),
        HeadlessCommandName::GetSettings => no_input(input, HeadlessCommand::GetSettings),
        HeadlessCommandName::SaveSettings => decode_input(input).map(HeadlessCommand::SaveSettings),
        HeadlessCommandName::CreateProject => {
            decode_input(input).map(HeadlessCommand::CreateProject)
        }
        HeadlessCommandName::OpenProject => decode_input(input).map(HeadlessCommand::OpenProject),
        HeadlessCommandName::CloseProject => no_input(input, HeadlessCommand::CloseProject),
        HeadlessCommandName::CurrentProject => no_input(input, HeadlessCommand::CurrentProject),
        HeadlessCommandName::GetTruthStatus => no_input(input, HeadlessCommand::GetTruthStatus),
        HeadlessCommandName::ImportTruth => no_input(input, HeadlessCommand::ImportTruth),
        HeadlessCommandName::GetFeatureCatalog => {
            no_input(input, HeadlessCommand::GetFeatureCatalog)
        }
        HeadlessCommandName::SubmitFeature => {
            decode_input(input).map(HeadlessCommand::SubmitFeature)
        }
        HeadlessCommandName::ListRuns => no_input(input, HeadlessCommand::ListRuns),
        HeadlessCommandName::GetRun => decode_input(input).map(HeadlessCommand::GetRun),
        HeadlessCommandName::ListExecutionGraphs => {
            no_input(input, HeadlessCommand::ListExecutionGraphs)
        }
        HeadlessCommandName::GetExecutionGraph => {
            decode_input(input).map(HeadlessCommand::GetExecutionGraph)
        }
        HeadlessCommandName::ResumeExecutionGraph => {
            decode_input(input).map(HeadlessCommand::ResumeExecutionGraph)
        }
        HeadlessCommandName::ListItemDefinitions => {
            no_input(input, HeadlessCommand::ListItemDefinitions)
        }
        HeadlessCommandName::GetItemDefinition => {
            decode_input(input).map(HeadlessCommand::GetItemDefinition)
        }
        HeadlessCommandName::SaveItemDefinition => {
            decode_input(input).map(HeadlessCommand::SaveItemDefinition)
        }
        HeadlessCommandName::ListCompositionDrafts => {
            no_input(input, HeadlessCommand::ListCompositionDrafts)
        }
        HeadlessCommandName::GetCompositionDraft => {
            decode_input(input).map(HeadlessCommand::GetCompositionDraft)
        }
        HeadlessCommandName::UpdateCompositionDraft => {
            decode_input(input).map(HeadlessCommand::UpdateCompositionDraft)
        }
        HeadlessCommandName::ConfirmCompositionDraft => {
            decode_input(input).map(HeadlessCommand::ConfirmCompositionDraft)
        }
        HeadlessCommandName::ListResourceAssets => {
            no_input(input, HeadlessCommand::ListResourceAssets)
        }
        HeadlessCommandName::SelectResource => {
            decode_input(input).map(HeadlessCommand::SelectResource)
        }
        HeadlessCommandName::SubmitCompositionItemFeedback => {
            decode_input(input).map(HeadlessCommand::SubmitCompositionItemFeedback)
        }
        HeadlessCommandName::Shutdown => no_input(input, HeadlessCommand::Shutdown),
    }
}

fn no_input(input: Option<Value>, command: HeadlessCommand) -> CommandResult<HeadlessCommand> {
    match input {
        None | Some(Value::Null) => Ok(command),
        Some(_) => Err(CommandFailure::invalid_input("headless.request.input")),
    }
}

fn decode_input<T: DeserializeOwned>(input: Option<Value>) -> CommandResult<T> {
    serde_json::from_value(
        input.ok_or_else(|| CommandFailure::invalid_input("headless.request.input"))?,
    )
    .map_err(|_| CommandFailure::invalid_input("headless.request.input"))
}

async fn dispatch(runtime: &HeadlessRuntime, command: HeadlessCommand) -> CommandResult<Value> {
    match command {
        HeadlessCommand::Health => value(health::health_report(
            &runtime.active,
            runtime.composition.as_ref(),
        )),
        HeadlessCommand::GetSettings => value(settings::settings_snapshot(&runtime.config)),
        HeadlessCommand::SaveSettings(input) => value(settings::save_settings_patch_inner(
            &runtime.config,
            input.patch,
        )?),
        HeadlessCommand::CreateProject(input) => value(
            project::create_project_inner(
                &runtime.active,
                &runtime.paths,
                &runtime.config,
                &runtime.composition,
                input.parent_dir,
                input.project_name,
            )
            .await?,
        ),
        HeadlessCommand::OpenProject(input) => value(
            project::open_project_inner(
                &runtime.active,
                &runtime.paths,
                &runtime.config,
                &runtime.composition,
                input.path,
            )
            .await?,
        ),
        HeadlessCommand::CloseProject => {
            project::close_project_inner(&runtime.active).await?;
            value(())
        }
        HeadlessCommand::CurrentProject => value(project::current_project_inner(&runtime.active)?),
        HeadlessCommand::GetTruthStatus => {
            value(stage2::get_truth_status_inner(&runtime.composition)?)
        }
        HeadlessCommand::ImportTruth => {
            value(stage2::import_truth_inner(Arc::clone(&runtime.composition)).await?)
        }
        HeadlessCommand::GetFeatureCatalog => value(stage2::get_feature_catalog()),
        HeadlessCommand::SubmitFeature(input) => value(
            stage2::submit_feature_inner(
                &runtime.active,
                Arc::clone(&runtime.composition),
                Arc::clone(&runtime.config),
                stage2::SubmitFeatureRequest {
                    feature_id: input.feature_id,
                    request: input.request,
                    source_path: input.source_path,
                },
            )
            .await?,
        ),
        HeadlessCommand::ListRuns => value(stage2::list_runs_inner(&runtime.active)?),
        HeadlessCommand::GetRun(input) => {
            value(stage2::get_run_inner(&runtime.active, input.run_id)?)
        }
        HeadlessCommand::ListExecutionGraphs => {
            value(stage2::list_execution_graphs_inner(&runtime.active)?)
        }
        HeadlessCommand::GetExecutionGraph(input) => value(stage2::get_execution_graph_inner(
            &runtime.active,
            input.execution_graph_id,
        )?),
        HeadlessCommand::ResumeExecutionGraph(input) => value(
            stage2::resume_execution_graph_inner(
                &runtime.active,
                Arc::clone(&runtime.composition),
                Arc::clone(&runtime.config),
                input.execution_graph_id,
                input.expected_revision,
            )
            .await?,
        ),
        HeadlessCommand::ListItemDefinitions => {
            value(stage2::list_item_definitions_inner(&runtime.active)?)
        }
        HeadlessCommand::GetItemDefinition(input) => value(stage2::get_item_definition_inner(
            &runtime.active,
            input.item_id,
            input.definition_hash,
        )?),
        HeadlessCommand::SaveItemDefinition(input) => value(stage2::save_item_definition_inner(
            &runtime.active,
            &runtime.composition,
            input.definition,
        )?),
        HeadlessCommand::ListCompositionDrafts => {
            value(stage2::list_composition_drafts_inner(&runtime.active)?)
        }
        HeadlessCommand::GetCompositionDraft(input) => value(stage2::get_composition_draft_inner(
            &runtime.active,
            input.draft_id,
        )?),
        HeadlessCommand::UpdateCompositionDraft(input) => {
            value(stage2::update_composition_draft_inner(
                &runtime.active,
                &runtime.composition,
                input.draft_id,
                input.expected_revision,
                input.nodes,
            )?)
        }
        HeadlessCommand::ConfirmCompositionDraft(input) => {
            value(stage2::confirm_composition_draft_inner(
                &runtime.active,
                &runtime.composition,
                input.draft_id,
                input.expected_revision,
                input.selected_item_ids,
            )?)
        }
        HeadlessCommand::ListResourceAssets => value(stage2::list_resource_assets_inner(
            &runtime.active,
            &runtime.composition,
        )?),
        HeadlessCommand::SelectResource(input) => value(stage2::select_resource_inner(
            &runtime.active,
            &runtime.composition,
            input.resource_id,
            input.version,
        )?),
        HeadlessCommand::SubmitCompositionItemFeedback(input) => value(
            stage2::submit_composition_item_feedback_inner(
                &runtime.active,
                Arc::clone(&runtime.composition),
                Arc::clone(&runtime.config),
                input.request,
            )
            .await?,
        ),
        HeadlessCommand::Shutdown => {
            project::close_project_inner(&runtime.active).await?;
            Ok(serde_json::json!({"shutdown": true}))
        }
    }
}

fn value<T: Serialize>(value: T) -> CommandResult<Value> {
    serde_json::to_value(value).map_err(|_| CommandFailure::unclassified("headless.response"))
}

fn valid_request_id(value: &str) -> bool {
    let length = value.chars().count();
    (1..=MAX_REQUEST_ID_CHARS).contains(&length)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn request_id_from_invalid_line(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| value.get("requestId")?.as_str().map(str::to_owned))
        .filter(|value| valid_request_id(value))
}

fn write_response(output: &mut impl Write, response: &HeadlessResponse) -> io::Result<()> {
    serde_json::to_writer(&mut *output, response)?;
    output.write_all(b"\n")?;
    output.flush()
}

async fn shutdown(runtime: &HeadlessRuntime, exit_code: i32) -> i32 {
    let _ = project::close_project_inner(&runtime.active).await;
    exit_code
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use super::*;

    #[test]
    fn request_ids_are_bounded_and_transport_safe() {
        assert!(valid_request_id("acceptance-001"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("contains space"));
        assert!(!valid_request_id(&"a".repeat(MAX_REQUEST_ID_CHARS + 1)));
    }

    #[test]
    fn oversized_lines_are_discarded_without_consuming_the_next_request() {
        let mut bytes = vec![b'a'; MAX_REQUEST_BYTES + 1];
        bytes.extend_from_slice(b"\n{}\n");
        let mut reader = BufReader::new(Cursor::new(bytes));
        let mut line = Vec::new();
        assert!(matches!(
            read_bounded_line(&mut reader, &mut line).unwrap(),
            BoundedLine::TooLarge
        ));
        assert!(line.is_empty());
        assert!(matches!(
            read_bounded_line(&mut reader, &mut line).unwrap(),
            BoundedLine::Line
        ));
        assert_eq!(line, b"{}\n");
    }

    #[test]
    fn malformed_requests_return_only_a_typed_failure() {
        let response = HeadlessResponse::failure(
            Some("request-1".into()),
            CommandFailure::invalid_input("headless.request.decode"),
        );
        let serialized = serde_json::to_value(response).unwrap();
        assert_eq!(serialized["ok"], false);
        assert_eq!(serialized["failure"]["code"], "run.input_invalid");
        assert!(serialized.get("diagnostic").is_none());
        assert!(serialized.get("request").is_none());
    }

    #[test]
    fn feedback_request_deserializes_without_becoming_a_response_field() {
        let line = format!(
            r#"{{"schemaVersion":1,"requestId":"feedback-1","command":{{"name":"submit_composition_item_feedback","input":{{"request":{{"executionGraphId":"graph-feedback","expectedRevision":1,"itemId":"card-feedback","expectedDefinitionHash":"{}","expectedBehaviorSha256":"{}","instruction":"change the behavior"}}}}}}}}"#,
            "a".repeat(64),
            "b".repeat(64),
        );
        let request: HeadlessRequest = serde_json::from_str(&line).unwrap();
        assert!(matches!(
            decode_command(request.command).unwrap(),
            HeadlessCommand::SubmitCompositionItemFeedback(_)
        ));
        let response = HeadlessResponse::success(
            request.request_id,
            serde_json::json!({
                "runId": "run-feedback"
            }),
        );
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("change the behavior"));
    }
}
