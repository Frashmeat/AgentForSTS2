//! Knowledge 状态数据类型。所有字段都安全可序列化给前端（不含密钥/绝对凭据）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OverallState {
    /// 反编译产物齐全且无 warning
    Fresh,
    /// 反编译产物部分/全部缺失
    Missing,
    /// 产物存在但需要更新（如版本不匹配；stage 2.1 暂未实现版本检测）
    Stale,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    /// 运行时已存在反编译源
    RuntimeDecompiled,
    /// 缺失，需要反编译/下载
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    pub source_mode: SourceMode,
    pub knowledge_path: String,
    /// 是否在 knowledge_path 下能找到至少一个 .cs 文件
    pub has_decompiled_sources: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselibStatus {
    pub source_mode: SourceMode,
    pub knowledge_path: String,
    /// BaseLib.decompiled.cs 文件是否存在
    pub has_decompiled_sources: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeStatus {
    pub overall: OverallState,
    pub knowledge_root: String,
    pub warnings: Vec<String>,
    pub game: GameStatus,
    pub baselib: BaselibStatus,
    /// 内嵌模板的 slot 名列表（不返回内容；内容通过 `get_template` 取）
    pub embedded_templates: Vec<String>,
}
