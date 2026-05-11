//! Codegen 请求载体。镜像 Python `codegen/domain/models.py`。
//!
//! 设计取向：尽量贴近 Python 字段名（snake_case），便于前端复用原 schema。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::planning::PlanItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AssetCodegenRequest {
    pub design_description: String,
    pub asset_type: String,
    pub asset_name: String,
    pub image_paths: Vec<PathBuf>,
    pub project_root: PathBuf,
    pub name_zhs: String,
    pub skip_build: bool,
}

impl Default for AssetCodegenRequest {
    fn default() -> Self {
        Self {
            design_description: String::new(),
            asset_type: String::new(),
            asset_name: String::new(),
            image_paths: Vec::new(),
            project_root: PathBuf::new(),
            name_zhs: String::new(),
            skip_build: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CustomCodegenRequest {
    pub description: String,
    pub implementation_notes: String,
    pub name: String,
    pub project_root: PathBuf,
    pub skip_build: bool,
}

impl Default for CustomCodegenRequest {
    fn default() -> Self {
        Self {
            description: String::new(),
            implementation_notes: String::new(),
            name: String::new(),
            project_root: PathBuf::new(),
            skip_build: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct AssetGroupItem {
    pub item: PlanItem,
    pub image_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct AssetGroupRequest {
    pub assets: Vec<AssetGroupItem>,
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct ModProjectRequest {
    pub project_name: String,
    pub target_dir: PathBuf,
}
