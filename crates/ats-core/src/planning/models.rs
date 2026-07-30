//! Planning 领域模型。镜像 Python `planning/domain/models.py`。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct AssetItemType(String);

impl AssetItemType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AssetItemType {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<String> for AssetItemType {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct PlanItem {
    pub id: String,
    /// Missing values deserialize to an empty ID so validation can report the
    /// field without choosing a game-specific fallback type.
    #[serde(rename = "type")]
    pub item_type: AssetItemType,
    pub name: String,
    pub name_zhs: String,
    pub description: String,
    pub goal: String,
    pub detailed_description: String,
    pub implementation_notes: String,
    pub needs_image: bool,
    pub image_description: String,
    pub depends_on_item_ids: Vec<String>,
    pub scope_boundary: String,
    pub relationship_reason: String,
    pub acceptance_notes: String,
    pub affected_targets: Vec<String>,
    pub relationship_type: String,
    pub clarification_status: String,
    pub clarification_questions: Vec<String>,
    pub provided_image_b64: String,
}

impl Default for PlanItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            item_type: AssetItemType::default(),
            name: String::new(),
            name_zhs: String::new(),
            description: String::new(),
            goal: String::new(),
            detailed_description: String::new(),
            implementation_notes: String::new(),
            needs_image: true,
            image_description: String::new(),
            depends_on_item_ids: Vec::new(),
            scope_boundary: String::new(),
            relationship_reason: String::new(),
            acceptance_notes: String::new(),
            affected_targets: Vec::new(),
            relationship_type: "unknown".into(),
            clarification_status: String::new(),
            clarification_questions: Vec::new(),
            provided_image_b64: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct ModPlan {
    pub mod_name: String,
    pub summary: String,
    pub items: Vec<PlanItem>,
}
