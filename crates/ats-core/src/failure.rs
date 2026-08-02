//! Shared actionable failure contract and fail-closed normalization.

use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::game_pack::{TruthSnapshotError, TruthSnapshotRefreshError};
use crate::image_gen::ImageGenError;
use crate::image_proc::ImageProcError;
use crate::llm::LlmError;
use crate::platform::artifact::ArtifactError;
use crate::platform::domain::{PackageError, RunError, RunId};
use crate::project::{LocalPropsError, ProjectError};
use crate::toolchain::GodotValidationError;

pub const FAILURE_SCHEMA_VERSION: u32 = 1;
pub const MAX_FAILURE_MESSAGE_CHARS: usize = 2_048;
pub const MAX_DIAGNOSTIC_SUMMARY_CHARS: usize = 512;
const MAX_CONTEXT_VALUE_CHARS: usize = 256;
const MAX_STAGE_CHARS: usize = 128;
const MAX_RETRY_AFTER_MS: u64 = 86_400_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    Configuration,
    Authentication,
    RateLimit,
    Network,
    Upstream,
    Validation,
    Filesystem,
    Toolchain,
    State,
    Internal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Configure,
    Reauthenticate,
    Retry,
    CheckPath,
    InstallDependency,
    OpenSettings,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureIoKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    TimedOut,
    ConnectionRefused,
    ConnectionReset,
    BrokenPipe,
    UnexpectedEof,
    Other,
}

impl From<std::io::ErrorKind> for FailureIoKind {
    fn from(value: std::io::ErrorKind) -> Self {
        match value {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
                Self::InvalidInput
            }
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            std::io::ErrorKind::ConnectionRefused => Self::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset => Self::ConnectionReset,
            std::io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            std::io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FailureContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency: Option<String>,
}

impl FailureContext {
    fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.http_status.is_none()
            && self.run_id.is_none()
            && self.project_relative_path.is_none()
            && self.setting_key.is_none()
            && self.dependency.is_none()
    }

    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("provider", self.provider.as_deref()),
            ("runId", self.run_id.as_deref()),
            ("settingKey", self.setting_key.as_deref()),
            ("dependency", self.dependency.as_deref()),
        ] {
            if let Some(value) = value
                && (value.trim().is_empty() || value.chars().count() > MAX_CONTEXT_VALUE_CHARS)
            {
                return Err(format!("failure context {name} is empty or too long"));
            }
        }
        if let Some(path) = &self.project_relative_path
            && (!is_safe_relative_path(path) || path.chars().count() > MAX_CONTEXT_VALUE_CHARS)
        {
            return Err("failure context projectRelativePath is unsafe or too long".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FailureDiagnostic {
    pub id: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_kind: Option<FailureIoKind>,
}

impl FailureDiagnostic {
    #[must_use]
    pub fn generated(summary: impl AsRef<str>, io_kind: Option<FailureIoKind>) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        Self {
            id: format!("diag-{nanos:032x}-{counter:08x}"),
            summary: bounded(summary.as_ref(), MAX_DIAGNOSTIC_SUMMARY_CHARS),
            io_kind,
        }
    }

    #[must_use]
    pub fn for_run(run_id: &RunId, summary: impl AsRef<str>) -> Self {
        Self {
            id: run_id.0.clone(),
            summary: bounded(summary.as_ref(), MAX_DIAGNOSTIC_SUMMARY_CHARS),
            io_kind: None,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if !is_safe_segment(&self.id) {
            return Err("failure diagnostic id must be a safe segment".into());
        }
        if self.summary.trim().is_empty()
            || self.summary.chars().count() > MAX_DIAGNOSTIC_SUMMARY_CHARS
        {
            return Err("failure diagnostic summary is empty or too long".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionableFailure {
    pub schema_version: u32,
    pub code: String,
    pub category: FailureCategory,
    pub stage: String,
    pub message: String,
    pub action: RecoveryAction,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<FailureContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<FailureDiagnostic>,
}

impl ActionableFailure {
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        category: FailureCategory,
        stage: impl Into<String>,
        message: impl AsRef<str>,
        action: RecoveryAction,
        retryable: bool,
    ) -> Self {
        Self {
            schema_version: FAILURE_SCHEMA_VERSION,
            code: code.into(),
            category,
            stage: bounded(&stage.into(), MAX_STAGE_CHARS),
            message: bounded(message.as_ref(), MAX_FAILURE_MESSAGE_CHARS),
            action,
            retryable,
            retry_after_ms: None,
            context: None,
            diagnostic: None,
        }
    }

    #[must_use]
    pub fn invalid_input(stage: impl Into<String>, message: &'static str) -> Self {
        Self::new(
            "run.input_invalid",
            FailureCategory::Validation,
            stage,
            message,
            RecoveryAction::None,
            false,
        )
    }

    #[must_use]
    pub fn interrupted(stage: impl Into<String>) -> Self {
        Self::new(
            "run.interrupted",
            FailureCategory::State,
            stage,
            "The application stopped before this run completed.",
            RecoveryAction::Retry,
            true,
        )
    }

    #[must_use]
    pub fn unclassified(stage: impl Into<String>) -> Self {
        Self::new(
            "core.unclassified",
            FailureCategory::Internal,
            stage,
            "An unexpected error occurred. Try again or review the diagnostic ID.",
            RecoveryAction::Retry,
            true,
        )
        .with_diagnostic(FailureDiagnostic::generated(
            "Unclassified internal failure; raw error text was not exposed.",
            None,
        ))
    }

    #[must_use]
    pub fn with_retry_after_ms(mut self, retry_after_ms: Option<u64>) -> Self {
        self.retry_after_ms = retry_after_ms.map(|value| value.min(MAX_RETRY_AFTER_MS));
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: FailureContext) -> Self {
        self.context = (!context.is_empty()).then_some(context);
        self
    }

    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: FailureDiagnostic) -> Self {
        self.diagnostic = Some(diagnostic);
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != FAILURE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported failure schema version {}",
                self.schema_version
            ));
        }
        if self.code.trim().is_empty()
            || !self.code.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._".contains(&byte)
            })
        {
            return Err("failure code is empty or unsafe".into());
        }
        if self.stage.trim().is_empty() || self.stage.chars().count() > MAX_STAGE_CHARS {
            return Err("failure stage is empty or too long".into());
        }
        if self.message.trim().is_empty()
            || self.message.chars().count() > MAX_FAILURE_MESSAGE_CHARS
        {
            return Err("failure message is empty or too long".into());
        }
        if self
            .retry_after_ms
            .is_some_and(|value| value > MAX_RETRY_AFTER_MS)
        {
            return Err("failure retryAfterMs exceeds the supported limit".into());
        }
        if let Some(context) = &self.context {
            context.validate()?;
        }
        if let Some(diagnostic) = &self.diagnostic {
            diagnostic.validate()?;
        }
        Ok(())
    }
}

pub struct FailureNormalizer;

impl FailureNormalizer {
    #[must_use]
    pub fn project_not_open(stage: &str) -> ActionableFailure {
        failure(
            "project.not_open",
            FailureCategory::State,
            stage,
            "Open or create a project before running this command.",
            RecoveryAction::None,
            false,
        )
    }

    #[must_use]
    pub fn io(
        code: &'static str,
        stage: &str,
        message: &'static str,
        error: &std::io::Error,
    ) -> ActionableFailure {
        io_failure(code, stage, message, error)
    }

    #[must_use]
    pub fn llm(stage: &str, error: &LlmError) -> ActionableFailure {
        match error {
            LlmError::Auth(_) => failure(
                "llm.authentication_failed",
                FailureCategory::Authentication,
                stage,
                "LLM authentication failed. Update the API credentials.",
                RecoveryAction::Reauthenticate,
                false,
            ),
            LlmError::RateLimit {
                retry_after_secs, ..
            } => failure(
                "llm.rate_limited",
                FailureCategory::RateLimit,
                stage,
                "The LLM provider rate-limited this request. Retry later.",
                RecoveryAction::Retry,
                true,
            )
            .with_retry_after_ms(retry_after_secs.and_then(|secs| secs.checked_mul(1_000))),
            LlmError::Transport(_) | LlmError::Stream(_) => failure(
                "llm.network_failed",
                FailureCategory::Network,
                stage,
                "The LLM provider could not be reached. Check the network and retry.",
                RecoveryAction::Retry,
                true,
            ),
            LlmError::Http {
                status: 401 | 403, ..
            } => failure(
                "llm.authentication_failed",
                FailureCategory::Authentication,
                stage,
                "LLM authentication failed. Update the API credentials.",
                RecoveryAction::Reauthenticate,
                false,
            ),
            LlmError::Http { status: 429, .. } => failure(
                "llm.rate_limited",
                FailureCategory::RateLimit,
                stage,
                "The LLM provider rate-limited this request. Retry later.",
                RecoveryAction::Retry,
                true,
            ),
            LlmError::Http { status, .. } => failure(
                "llm.upstream_failed",
                FailureCategory::Upstream,
                stage,
                "The LLM provider rejected or failed the request.",
                RecoveryAction::Retry,
                (500..=599).contains(status),
            )
            .with_context(FailureContext {
                http_status: Some(*status),
                ..FailureContext::default()
            }),
            LlmError::Parse(_) => failure(
                "llm.upstream_failed",
                FailureCategory::Upstream,
                stage,
                "The LLM provider returned an invalid response.",
                RecoveryAction::Retry,
                true,
            ),
            LlmError::Cancelled => failure(
                "llm.network_failed",
                FailureCategory::State,
                stage,
                "The LLM request was cancelled before completion.",
                RecoveryAction::Retry,
                true,
            ),
            LlmError::Config(_) => failure(
                "llm.config_missing",
                FailureCategory::Configuration,
                stage,
                "LLM configuration is incomplete. Open Settings and configure the provider.",
                RecoveryAction::OpenSettings,
                false,
            )
            .with_context(FailureContext {
                setting_key: Some("llm".into()),
                ..FailureContext::default()
            }),
        }
    }

    #[must_use]
    pub fn image(stage: &str, error: &ImageGenError) -> ActionableFailure {
        match error {
            ImageGenError::Auth(_) => failure(
                "image.authentication_failed",
                FailureCategory::Authentication,
                stage,
                "Image provider authentication failed. Update the API credentials.",
                RecoveryAction::Reauthenticate,
                false,
            ),
            ImageGenError::RateLimit {
                retry_after_secs, ..
            } => failure(
                "image.rate_limited",
                FailureCategory::RateLimit,
                stage,
                "The image provider rate-limited this request. Retry later.",
                RecoveryAction::Retry,
                true,
            )
            .with_retry_after_ms(retry_after_secs.and_then(|secs| secs.checked_mul(1_000))),
            ImageGenError::Transport(_) => failure(
                "image.network_failed",
                FailureCategory::Network,
                stage,
                "The image provider could not be reached. Check the network and retry.",
                RecoveryAction::Retry,
                true,
            ),
            ImageGenError::Http {
                status: 401 | 403, ..
            } => failure(
                "image.authentication_failed",
                FailureCategory::Authentication,
                stage,
                "Image provider authentication failed. Update the API credentials.",
                RecoveryAction::Reauthenticate,
                false,
            ),
            ImageGenError::Http { status: 429, .. } => failure(
                "image.rate_limited",
                FailureCategory::RateLimit,
                stage,
                "The image provider rate-limited this request. Retry later.",
                RecoveryAction::Retry,
                true,
            ),
            ImageGenError::Http { status, .. } => failure(
                "image.upstream_failed",
                FailureCategory::Upstream,
                stage,
                "The image provider rejected or failed the request.",
                RecoveryAction::Retry,
                (500..=599).contains(status),
            )
            .with_context(FailureContext {
                http_status: Some(*status),
                ..FailureContext::default()
            }),
            ImageGenError::Parse(_) => failure(
                "image.upstream_failed",
                FailureCategory::Upstream,
                stage,
                "The image provider returned invalid image data.",
                RecoveryAction::Retry,
                true,
            ),
            ImageGenError::Config(_) => failure(
                "image.config_missing",
                FailureCategory::Configuration,
                stage,
                "Image generation is not configured. Open Settings and configure the provider.",
                RecoveryAction::OpenSettings,
                false,
            ),
            ImageGenError::Empty => failure(
                "image.empty_result",
                FailureCategory::Upstream,
                stage,
                "The image provider returned no usable image.",
                RecoveryAction::Retry,
                true,
            ),
        }
    }

    #[must_use]
    pub fn project(stage: &str, error: &ProjectError) -> ActionableFailure {
        match error {
            ProjectError::Locked(_) => failure(
                "project.locked",
                FailureCategory::State,
                stage,
                "The project is open in another process.",
                RecoveryAction::Retry,
                true,
            ),
            ProjectError::NotADirectory(_)
            | ProjectError::InvalidName(_)
            | ProjectError::Missing(_)
            | ProjectError::InvalidSchemaVersion { .. }
            | ProjectError::UnsupportedSchemaVersion { .. }
            | ProjectError::InvalidHistorySchema { .. } => failure(
                "project.path_invalid",
                FailureCategory::Validation,
                stage,
                "The selected project path or project data is invalid.",
                RecoveryAction::CheckPath,
                false,
            ),
            ProjectError::AlreadyExists(_) => failure(
                "project.path_invalid",
                FailureCategory::State,
                stage,
                "A project already exists at the selected location.",
                RecoveryAction::CheckPath,
                false,
            ),
            ProjectError::Io(error) => io_failure(
                "project.path_invalid",
                stage,
                "The project files could not be accessed.",
                error,
            ),
            ProjectError::MissingGameId
            | ProjectError::UnknownGameId(_)
            | ProjectError::ScaffoldContract(_)
            | ProjectError::Json(_) => failure(
                "project.path_invalid",
                FailureCategory::Validation,
                stage,
                "The project data does not satisfy the current project contract.",
                RecoveryAction::CheckPath,
                false,
            ),
            ProjectError::GamePackRegistry(_) => ActionableFailure::unclassified(stage),
        }
    }

    #[must_use]
    pub fn run(stage: &str, error: &RunError) -> ActionableFailure {
        match error {
            RunError::NotFound(id) => failure(
                "run.not_found",
                FailureCategory::State,
                stage,
                "The requested run no longer exists.",
                RecoveryAction::None,
                false,
            )
            .with_context(FailureContext {
                run_id: safe_segment(id).then(|| id.clone()),
                ..FailureContext::default()
            }),
            RunError::Io(error) => io_failure(
                "run.storage_failed",
                stage,
                "Run history could not be read or written.",
                error,
            ),
            RunError::Json(_) | RunError::Storage(_) | RunError::InvalidRecord { .. } => failure(
                "run.storage_failed",
                FailureCategory::Filesystem,
                stage,
                "Run history is unavailable or invalid.",
                RecoveryAction::Retry,
                true,
            ),
            RunError::AlreadyExists(_) | RunError::InvalidTransition { .. } => failure(
                "run.invalid_transition",
                FailureCategory::State,
                stage,
                "The run state changed and this operation can no longer be applied.",
                RecoveryAction::None,
                false,
            ),
        }
    }

    #[must_use]
    pub fn artifact(stage: &str, error: &ArtifactError) -> ActionableFailure {
        match error {
            ArtifactError::Io(error) => {
                let mut normalized = io_failure(
                    "artifact.publish_failed",
                    stage,
                    "The artifact snapshot could not be published.",
                    error,
                );
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    normalized.action = RecoveryAction::Retry;
                    normalized.retryable = true;
                }
                normalized
            }
            ArtifactError::AlreadyExists(_) => failure(
                "artifact.snapshot_exists",
                FailureCategory::State,
                stage,
                "An artifact snapshot already exists for this run.",
                RecoveryAction::None,
                false,
            ),
            ArtifactError::UnsafeId(_)
            | ArtifactError::UnsafeRelativePath(_)
            | ArtifactError::NotRegularFile(_)
            | ArtifactError::Symlink(_) => failure(
                "artifact.path_invalid",
                FailureCategory::Validation,
                stage,
                "The artifact snapshot contains an invalid or unsafe path.",
                RecoveryAction::CheckPath,
                false,
            ),
            ArtifactError::InvalidManifest(_) | ArtifactError::Json(_) => failure(
                "artifact.manifest_invalid",
                FailureCategory::Internal,
                stage,
                "The artifact manifest could not be created or validated.",
                RecoveryAction::Retry,
                true,
            ),
        }
    }

    #[must_use]
    pub fn truth_snapshot(stage: &str, error: &TruthSnapshotRefreshError) -> ActionableFailure {
        match error {
            TruthSnapshotRefreshError::Cancelled => ActionableFailure::interrupted(stage),
            TruthSnapshotRefreshError::Busy(_) => failure(
                "truth_snapshot.busy",
                FailureCategory::State,
                stage,
                "A Truth Snapshot refresh is already running for this Game Pack.",
                RecoveryAction::Retry,
                true,
            ),
            TruthSnapshotRefreshError::MissingLocalInput { .. }
            | TruthSnapshotRefreshError::LocalInputNotFile { .. } => failure(
                "truth_snapshot.input_invalid",
                FailureCategory::Configuration,
                stage,
                "A required Truth Snapshot source is missing or invalid.",
                RecoveryAction::OpenSettings,
                false,
            ),
            TruthSnapshotRefreshError::Fetch { .. } => failure(
                "truth_snapshot.fetch_failed",
                FailureCategory::Network,
                stage,
                "A declared Truth Snapshot source could not be downloaded.",
                RecoveryAction::Retry,
                true,
            ),
            TruthSnapshotRefreshError::Index { .. } | TruthSnapshotRefreshError::Tool(_) => {
                failure(
                    "truth_snapshot.index_failed",
                    FailureCategory::Toolchain,
                    stage,
                    "A Truth Snapshot source could not be indexed. Check the local indexer and source.",
                    RecoveryAction::InstallDependency,
                    false,
                )
            }
            TruthSnapshotRefreshError::Worker(_) => ActionableFailure::unclassified(stage),
            TruthSnapshotRefreshError::Snapshot(TruthSnapshotError::Io { source, .. })
            | TruthSnapshotRefreshError::Io { source, .. } => {
                let locked = source.kind() == std::io::ErrorKind::PermissionDenied;
                let mut normalized = io_failure(
                    if locked {
                        "truth_snapshot.storage_locked"
                    } else {
                        "truth_snapshot.storage_failed"
                    },
                    stage,
                    if locked {
                        "Truth Snapshot storage is locked or not writable. Close tools watching the runtime directory and retry."
                    } else {
                        "Truth Snapshot storage could not be read or updated."
                    },
                    source,
                );
                if locked {
                    normalized.action = RecoveryAction::CheckPath;
                    normalized.retryable = false;
                }
                normalized
            }
            TruthSnapshotRefreshError::Snapshot(_) => failure(
                "truth_snapshot.invalid",
                FailureCategory::Validation,
                stage,
                "The refreshed Truth Snapshot did not satisfy its integrity contract.",
                RecoveryAction::Retry,
                false,
            ),
        }
    }

    #[must_use]
    pub fn toolchain(stage: &str, error: &GodotValidationError) -> ActionableFailure {
        match error {
            GodotValidationError::NotAFile(_) => failure(
                "toolchain.not_found",
                FailureCategory::Toolchain,
                stage,
                "The configured Godot executable was not found.",
                RecoveryAction::OpenSettings,
                false,
            ),
            GodotValidationError::Spawn { source, .. }
            | GodotValidationError::Output { source, .. } => io_failure(
                "toolchain.version_failed",
                stage,
                "Godot could not be started or inspected.",
                source,
            ),
            GodotValidationError::Timeout { .. }
            | GodotValidationError::VersionCommandFailed { .. } => failure(
                "toolchain.version_failed",
                FailureCategory::Toolchain,
                stage,
                "Godot version validation failed. Check the executable and retry.",
                RecoveryAction::CheckPath,
                true,
            ),
            GodotValidationError::UnsupportedVersion { .. } => failure(
                "toolchain.version_failed",
                FailureCategory::Toolchain,
                stage,
                "Godot 4.5.1 is required. Select a supported executable.",
                RecoveryAction::OpenSettings,
                false,
            ),
        }
    }

    #[must_use]
    pub fn local_props(stage: &str, error: &LocalPropsError) -> ActionableFailure {
        match error {
            LocalPropsError::MissingInput(key) => failure(
                "toolchain.not_configured",
                FailureCategory::Configuration,
                stage,
                "A required local toolchain input is not configured.",
                RecoveryAction::OpenSettings,
                false,
            )
            .with_context(FailureContext {
                setting_key: safe_context_value(key).then(|| key.clone()),
                ..FailureContext::default()
            }),
            LocalPropsError::Io(error) => io_failure(
                "toolchain.not_configured",
                stage,
                "The local build configuration could not be updated.",
                error,
            ),
            LocalPropsError::MissingBuildRecipe
            | LocalPropsError::MissingPropertyGroup
            | LocalPropsError::Xml(_) => failure(
                "toolchain.not_configured",
                FailureCategory::Toolchain,
                stage,
                "The local build configuration is incomplete or invalid.",
                RecoveryAction::OpenSettings,
                false,
            ),
        }
    }

    #[must_use]
    pub fn image_proc(stage: &str, error: &ImageProcError) -> ActionableFailure {
        match error {
            ImageProcError::Cancelled => ActionableFailure::interrupted(stage),
            ImageProcError::NotReady(_) => failure(
                "image_proc.not_ready",
                FailureCategory::Configuration,
                stage,
                "Image processing is not ready. Install or repair its runtime dependency.",
                RecoveryAction::InstallDependency,
                false,
            )
            .with_context(FailureContext {
                dependency: Some("onnxruntime".into()),
                ..FailureContext::default()
            }),
            ImageProcError::Model(_) => failure(
                "image_proc.model_failed",
                FailureCategory::Toolchain,
                stage,
                "The image processing model could not be loaded.",
                RecoveryAction::Retry,
                true,
            ),
            ImageProcError::Runtime(_)
            | ImageProcError::Decode(_)
            | ImageProcError::Encode(_)
            | ImageProcError::Unsupported(_)
            | ImageProcError::Quality(_) => failure(
                "image_proc.runtime_failed",
                FailureCategory::Validation,
                stage,
                "Image processing could not produce a valid deliverable image.",
                RecoveryAction::Retry,
                false,
            ),
        }
    }

    #[must_use]
    pub fn package(stage: &str, error: &PackageError) -> ActionableFailure {
        match error {
            PackageError::SourceMissing => failure(
                "package.source_missing",
                FailureCategory::Validation,
                stage,
                "The package source directory does not exist.",
                RecoveryAction::CheckPath,
                false,
            ),
            PackageError::OutputInvalid => failure(
                "package.output_invalid",
                FailureCategory::Validation,
                stage,
                "The package output path must be a writable ZIP file path.",
                RecoveryAction::CheckPath,
                false,
            ),
            PackageError::RequiredFileMissing { relative_path } => failure(
                "package.required_file_missing",
                FailureCategory::Validation,
                stage,
                "A file required by the Game Pack is missing.",
                RecoveryAction::CheckPath,
                false,
            )
            .with_context(package_path_context(relative_path)),
            PackageError::PathEscape { relative_path }
            | PackageError::Symlink { relative_path } => failure(
                "package.path_escape",
                FailureCategory::Validation,
                stage,
                "A declared package path is outside the allowed source tree.",
                RecoveryAction::CheckPath,
                false,
            )
            .with_context(package_path_context(relative_path)),
            PackageError::NotRegularFile { relative_path } => failure(
                "package.required_file_missing",
                FailureCategory::Validation,
                stage,
                "A required package path is not a regular file.",
                RecoveryAction::CheckPath,
                false,
            )
            .with_context(package_path_context(relative_path)),
            PackageError::Io { source, .. } => {
                let code = if source.kind() == std::io::ErrorKind::PermissionDenied {
                    "package.permission_denied"
                } else {
                    "package.output_invalid"
                };
                io_failure(
                    code,
                    stage,
                    "Package files could not be read or written.",
                    source,
                )
            }
            PackageError::Zip | PackageError::Worker => failure(
                "package.output_invalid",
                FailureCategory::Internal,
                stage,
                "The package could not be completed.",
                RecoveryAction::Retry,
                true,
            ),
        }
    }
}

pub struct FailureSanitizer;

impl FailureSanitizer {
    #[must_use]
    pub fn redact_known(text: &str, secrets: &[&str], project_root: Option<&Path>) -> String {
        let mut safe = text.to_owned();
        for secret in secrets.iter().copied().filter(|secret| !secret.is_empty()) {
            safe = safe.replace(secret, "[REDACTED]");
        }
        if let Some(root) = project_root {
            let root = root.to_string_lossy();
            if !root.is_empty() {
                safe = safe.replace(root.as_ref(), "<project>");
            }
        }
        safe = safe
            .split_whitespace()
            .map(sanitize_token)
            .collect::<Vec<_>>()
            .join(" ");
        bounded(&safe, MAX_DIAGNOSTIC_SUMMARY_CHARS)
    }

    #[must_use]
    pub fn project_relative(project_root: &Path, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(project_root).ok()?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(value) => parts.push(value.to_str()?.to_owned()),
                _ => return None,
            }
        }
        let value = parts.join("/");
        (!value.is_empty() && is_safe_relative_path(&value)).then_some(value)
    }
}

fn sanitize_token(token: &str) -> String {
    let trimmed = token.trim_matches(|character: char| {
        matches!(character, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';')
    });
    if let Ok(mut url) = reqwest::Url::parse(trimmed) {
        url.set_query(None);
        url.set_fragment(None);
        return url.to_string();
    }
    token.to_owned()
}

fn failure(
    code: &str,
    category: FailureCategory,
    stage: &str,
    message: &str,
    action: RecoveryAction,
    retryable: bool,
) -> ActionableFailure {
    ActionableFailure::new(code, category, stage, message, action, retryable)
}

fn package_path_context(relative_path: &str) -> FailureContext {
    FailureContext {
        project_relative_path: (relative_path.chars().count() <= MAX_CONTEXT_VALUE_CHARS
            && is_safe_relative_path(relative_path))
        .then(|| relative_path.to_owned()),
        ..FailureContext::default()
    }
}

fn io_failure(code: &str, stage: &str, message: &str, error: &std::io::Error) -> ActionableFailure {
    let io_kind = FailureIoKind::from(error.kind());
    let action = match io_kind {
        FailureIoKind::NotFound
        | FailureIoKind::PermissionDenied
        | FailureIoKind::AlreadyExists
        | FailureIoKind::InvalidInput => RecoveryAction::CheckPath,
        FailureIoKind::TimedOut
        | FailureIoKind::ConnectionRefused
        | FailureIoKind::ConnectionReset
        | FailureIoKind::BrokenPipe
        | FailureIoKind::UnexpectedEof
        | FailureIoKind::Other => RecoveryAction::Retry,
    };
    failure(
        code,
        FailureCategory::Filesystem,
        stage,
        message,
        action,
        !matches!(
            io_kind,
            FailureIoKind::PermissionDenied
                | FailureIoKind::AlreadyExists
                | FailureIoKind::InvalidInput
        ),
    )
    .with_diagnostic(FailureDiagnostic::generated(
        "I/O operation failed; raw path and OS message were not exposed.",
        Some(io_kind),
    ))
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn safe_segment(value: &str) -> bool {
    is_safe_segment(value) && value.chars().count() <= MAX_CONTEXT_VALUE_CHARS
}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn safe_context_value(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_CONTEXT_VALUE_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains(':')
        && !value.starts_with('/')
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_auth_and_rate_limit_are_stable_and_do_not_leak_provider_body() {
        let canary = "secret-provider-body";
        let auth = FailureNormalizer::llm("llm.complete", &LlmError::Auth(canary.into()));
        assert_eq!(auth.code, "llm.authentication_failed");
        assert_eq!(auth.category, FailureCategory::Authentication);
        assert_eq!(auth.action, RecoveryAction::Reauthenticate);
        assert!(!serde_json::to_string(&auth).unwrap().contains(canary));

        let rate = FailureNormalizer::llm(
            "llm.complete",
            &LlmError::RateLimit {
                retry_after_secs: Some(17),
                message: canary.into(),
            },
        );
        assert_eq!(rate.retry_after_ms, Some(17_000));
        assert!(!serde_json::to_string(&rate).unwrap().contains(canary));
    }

    #[test]
    fn unknown_failure_never_uses_raw_display_text() {
        let failure = ActionableFailure::unclassified("tauri.command");
        let serialized = serde_json::to_string(&failure).unwrap();
        assert_eq!(failure.code, "core.unclassified");
        assert!(failure.retryable);
        assert!(!serialized.contains("C:\\Users\\private"));
        assert!(failure.diagnostic.is_some());
        assert!(failure.validate().is_ok());
    }

    #[test]
    fn sanitizer_removes_known_secrets_project_root_and_url_query() {
        let root = Path::new("C:\\Users\\private\\project");
        let value =
            "token-123 C:\\Users\\private\\project https://example.test/path?token=canary#frag";
        let safe = FailureSanitizer::redact_known(value, &["token-123"], Some(root));
        assert!(!safe.contains("token-123"));
        assert!(!safe.contains("private"));
        assert!(!safe.contains("canary"));
        assert!(!safe.contains("frag"));
        assert!(safe.contains("https://example.test/path"));
    }

    #[test]
    fn project_relative_paths_are_normalized_and_bounded() {
        let root = Path::new("C:\\project");
        assert_eq!(
            FailureSanitizer::project_relative(root, &root.join("Generated/Card.cs")),
            Some("Generated/Card.cs".into())
        );
        assert!(FailureSanitizer::project_relative(root, Path::new("C:\\outside\\x")).is_none());
    }

    #[test]
    fn io_failure_keeps_only_stable_kind() {
        let error = ProjectError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "C:\\Users\\private\\secret.txt",
        ));
        let failure = FailureNormalizer::project("project.open", &error);
        let serialized = serde_json::to_string(&failure).unwrap();
        assert_eq!(failure.code, "project.path_invalid");
        assert_eq!(failure.category, FailureCategory::Filesystem);
        assert_eq!(failure.action, RecoveryAction::CheckPath);
        assert!(!serialized.contains("private"));
        assert_eq!(
            failure.diagnostic.unwrap().io_kind,
            Some(FailureIoKind::PermissionDenied)
        );
    }

    #[test]
    fn artifact_publish_failure_keeps_io_kind_without_raw_error() {
        let canary = "C:\\Users\\private\\artifact.lock";
        let error = ArtifactError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            canary,
        ));
        let failure = FailureNormalizer::artifact("asset_generate.publish", &error);
        let serialized = serde_json::to_string(&failure).unwrap();

        assert_eq!(failure.code, "artifact.publish_failed");
        assert_eq!(failure.category, FailureCategory::Filesystem);
        assert_eq!(failure.action, RecoveryAction::Retry);
        assert!(failure.retryable);
        assert_eq!(
            failure.diagnostic.unwrap().io_kind,
            Some(FailureIoKind::PermissionDenied)
        );
        assert!(!serialized.contains("private"));
        assert!(!serialized.contains("artifact.lock"));
    }

    #[test]
    fn artifact_path_failure_does_not_expose_absolute_path() {
        let canary = "C:\\Users\\private\\artifacts";
        let failure = FailureNormalizer::artifact(
            "package.publish",
            &ArtifactError::NotRegularFile(canary.into()),
        );
        let serialized = serde_json::to_string(&failure).unwrap();

        assert_eq!(failure.code, "artifact.path_invalid");
        assert_eq!(failure.category, FailureCategory::Validation);
        assert_eq!(failure.action, RecoveryAction::CheckPath);
        assert!(!failure.retryable);
        assert!(!serialized.contains("private"));
    }

    #[test]
    fn truth_snapshot_lock_is_typed_and_redacted() {
        let canary = "C:\\Users\\private\\runtime\\game-packs\\fixture";
        let error = TruthSnapshotRefreshError::Io {
            action: "activate immutable truth snapshot directory",
            path: Path::new(canary).to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, canary),
        };
        let failure = FailureNormalizer::truth_snapshot("truth_snapshot.refresh", &error);
        let serialized = serde_json::to_string(&failure).unwrap();

        assert_eq!(failure.code, "truth_snapshot.storage_locked");
        assert_eq!(failure.category, FailureCategory::Filesystem);
        assert_eq!(failure.action, RecoveryAction::CheckPath);
        assert!(!failure.retryable);
        assert_eq!(
            failure.diagnostic.unwrap().io_kind,
            Some(FailureIoKind::PermissionDenied)
        );
        assert!(!serialized.contains("private"));
        assert!(!serialized.contains("game-packs"));
    }

    #[test]
    fn image_proc_variants_keep_stable_categories_without_internal_text() {
        let canary = "C:\\Users\\private\\model.onnx?token=secret";
        let not_ready = FailureNormalizer::image_proc(
            "image.remove_background",
            &ImageProcError::NotReady(canary.into()),
        );
        assert_eq!(not_ready.code, "image_proc.not_ready");
        assert_eq!(not_ready.action, RecoveryAction::InstallDependency);
        assert!(!serde_json::to_string(&not_ready).unwrap().contains(canary));

        let model = FailureNormalizer::image_proc(
            "image.remove_background",
            &ImageProcError::Model(canary.into()),
        );
        assert_eq!(model.code, "image_proc.model_failed");

        let runtime = FailureNormalizer::image_proc(
            "image.remove_background",
            &ImageProcError::Runtime(canary.into()),
        );
        assert_eq!(runtime.code, "image_proc.runtime_failed");
        assert!(!serde_json::to_string(&runtime).unwrap().contains(canary));
    }

    #[test]
    fn package_permission_and_declared_path_are_normalized_without_absolute_paths() {
        let permission = FailureNormalizer::package(
            "package.write",
            &PackageError::Io {
                operation: "create_output",
                source: std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "C:\\Users\\private\\mods.zip",
                ),
            },
        );
        assert_eq!(permission.code, "package.permission_denied");
        assert_eq!(permission.action, RecoveryAction::CheckPath);
        assert!(
            !serde_json::to_string(&permission)
                .unwrap()
                .contains("private")
        );

        let missing = FailureNormalizer::package(
            "package.layout",
            &PackageError::RequiredFileMissing {
                relative_path: "runtime/core.bin".into(),
            },
        );
        assert_eq!(missing.code, "package.required_file_missing");
        assert_eq!(
            missing.context.unwrap().project_relative_path.as_deref(),
            Some("runtime/core.bin")
        );
    }

    #[test]
    fn network_and_toolchain_failures_keep_stable_categories_without_raw_details() {
        let canary = "C:\\Users\\private\\Godot.exe?token=secret";
        let network = FailureNormalizer::llm("llm.complete", &LlmError::Transport(canary.into()));
        assert_eq!(network.code, "llm.network_failed");
        assert_eq!(network.category, FailureCategory::Network);
        assert!(network.retryable);
        assert!(!serde_json::to_string(&network).unwrap().contains(canary));

        let toolchain = FailureNormalizer::toolchain(
            "toolchain.validate",
            &GodotValidationError::NotAFile(Path::new(canary).to_path_buf()),
        );
        assert_eq!(toolchain.code, "toolchain.not_found");
        assert_eq!(toolchain.category, FailureCategory::Toolchain);
        assert_eq!(toolchain.action, RecoveryAction::OpenSettings);
        assert!(
            !serde_json::to_string(&toolchain)
                .unwrap()
                .contains("private")
        );
    }
}
