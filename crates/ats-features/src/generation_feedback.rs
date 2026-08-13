use ats_kernel::{SchemaId, SchemaRef, SchemaVersion, Sha256Digest};
use ats_runtime::ValidationIssue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationFeedbackPhase {
    OutputContract,
    GeneratedContent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationFeedbackMode {
    RegenerateCompleteBundle,
    ReplaceCompleteRoles,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputContractDiagnosticCode {
    OutputTruncated,
    JsonDecode,
    FileCount,
    AcceptanceNotes,
    FileRole,
    FileContent,
    MergeShape,
    MergeKeyConflict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ObservedJsonShape {
    Missing,
    String,
    JsonEncodedString,
    Object,
    ObjectWithNestedValue,
    ObjectWithNonStringValue,
    Array,
    Number,
    Boolean,
    Null,
    Unparseable,
    Truncated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedOutputShape {
    CompleteBundle,
    ExactDeclaredRoles,
    BoundedStringArray,
    NonEmptyString,
    FlatStringObject,
    UniqueObjectKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputContractDiagnostic {
    pub code: OutputContractDiagnosticCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<String>,
    pub expected_shape: ExpectedOutputShape,
    pub observed_shape: ObservedJsonShape,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationFeedbackEnvelope {
    pub phase: GenerationFeedbackPhase,
    pub mode: GenerationFeedbackMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_diagnostics: Vec<OutputContractDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_issues: Vec<ValidationIssue>,
}

impl GenerationFeedbackEnvelope {
    #[must_use]
    pub fn output_contract(diagnostic: OutputContractDiagnostic) -> Self {
        Self {
            phase: GenerationFeedbackPhase::OutputContract,
            mode: GenerationFeedbackMode::RegenerateCompleteBundle,
            output_diagnostics: vec![diagnostic],
            validation_issues: Vec::new(),
        }
    }

    #[must_use]
    pub fn generated_content(issues: &[ValidationIssue]) -> Self {
        Self {
            phase: GenerationFeedbackPhase::GeneratedContent,
            mode: GenerationFeedbackMode::ReplaceCompleteRoles,
            output_diagnostics: Vec::new(),
            validation_issues: issues.to_vec(),
        }
    }

    #[must_use]
    pub fn repair_output_contract(
        diagnostic: OutputContractDiagnostic,
        issues: &[ValidationIssue],
    ) -> Self {
        Self {
            phase: GenerationFeedbackPhase::OutputContract,
            mode: GenerationFeedbackMode::ReplaceCompleteRoles,
            output_diagnostics: vec![diagnostic],
            validation_issues: issues.to_vec(),
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        match (self.phase, self.mode) {
            (
                GenerationFeedbackPhase::OutputContract,
                GenerationFeedbackMode::RegenerateCompleteBundle,
            ) => self.output_diagnostics.len() == 1 && self.validation_issues.is_empty(),
            (
                GenerationFeedbackPhase::GeneratedContent,
                GenerationFeedbackMode::ReplaceCompleteRoles,
            ) => self.output_diagnostics.is_empty() && !self.validation_issues.is_empty(),
            (
                GenerationFeedbackPhase::OutputContract,
                GenerationFeedbackMode::ReplaceCompleteRoles,
            ) => self.output_diagnostics.len() == 1 && !self.validation_issues.is_empty(),
            _ => false,
        }
    }
}

pub fn generation_feedback_schema() -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse("feature.generation-feedback")
            .expect("built-in feedback schema ID is valid"),
        version: SchemaVersion::new(2).expect("built-in feedback schema version is valid"),
    }
}

pub fn diagnostic_fingerprint(
    envelope: &GenerationFeedbackEnvelope,
) -> Result<Sha256Digest, serde_json::Error> {
    let bytes = serde_json::to_vec(envelope)?;
    Ok(Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 output is a valid digest"))
}

#[must_use]
pub fn candidate_sha256(content: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(content)))
        .expect("SHA-256 output is a valid digest")
}
