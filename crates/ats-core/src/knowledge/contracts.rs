//! Knowledge "context packet" 数据契约——跨模块共享的载体类型。
//!
//! 镜像 Python `backend/app/shared/contracts/knowledge.py`。这些类型由下游
//! `Sts2KnowledgeResolver` 等 provider 填充，再交给 `prompting::PromptContextAssembler`
//! 渲染成 LLM prompt 上下文。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeScenario {
    Planner,
    AssetCodegen,
    CustomCodeCodegen,
    AssetGroupCodegen,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct KnowledgeQuery {
    pub scenario: Option<KnowledgeScenario>,
    pub domain: String,
    pub asset_type: Option<String>,
    pub project_root: Option<PathBuf>,
    pub requirements: Option<String>,
    pub item_name: Option<String>,
    pub symbols: Vec<String>,
    pub group_asset_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct KnowledgeFactItem {
    pub key: String,
    pub title: String,
    pub body: String,
    pub priority: i32,
    pub evidence_paths: Vec<String>,
    pub keywords: Vec<String>,
    pub asset_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct KnowledgeGuidanceItem {
    pub key: String,
    pub title: String,
    pub body: String,
    pub source_path: String,
    pub asset_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct KnowledgeLookupItem {
    pub key: String,
    pub title: String,
    pub path: String,
    pub note: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct KnowledgePacket {
    pub domain: String,
    pub scenario: String,
    pub summary: String,
    pub facts: Vec<KnowledgeFactItem>,
    pub guidance: Vec<KnowledgeGuidanceItem>,
    pub lookup: Vec<KnowledgeLookupItem>,
    pub warnings: Vec<String>,
}
