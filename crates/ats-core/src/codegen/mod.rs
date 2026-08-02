//! Codegen module — LLM prompt 装配（不含执行）。
//!
//! 镜像 Python `app/modules/codegen`，但只移植 prompt 装配部分；
//! agent_runner / artifact_writer / build_trigger 等需要 LLM 客户端的部分留待
//! `ats-core::llm` 就绪后再做（路线图 stage 4）。
//!
//! 当前能力：
//! - 把 codegen request（asset / custom_code / asset_group / mod_project / build / package）
//!   组装成完整 prompt 字符串，可直接交给 LLM。
//! - prompt 模板来自内嵌的 `codegen.md`（PromptLoader bundle），知识上下文来自
//!   `Sts2KnowledgeResolver`。

mod models;
mod prompt_assembler;
mod validation;

pub(crate) use models::asset_localization_key_segment;
pub use models::{
    AssetCodegenRequest, AssetGroupItem, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
};
pub use prompt_assembler::{
    AssetPromptAssembly, GenerationEvidence, PromptAssembler, PromptAssemblyError,
};
pub(crate) use validation::{validate_generated_csharp, validate_localization_rich_text};
