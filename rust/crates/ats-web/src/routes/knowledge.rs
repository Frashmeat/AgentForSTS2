//! Knowledge HTTP 路由——对标 `backend/routers/knowledge_router.py`。

use std::sync::Arc;

use ats_core::knowledge::{KnowledgePaths, KnowledgeStatus, get_status};
use axum::{Extension, Json, Router, routing::{get, post}};

use crate::AppState;

pub fn router() -> Router {
    Router::new()
        .route("/api/knowledge/status", get(status_handler))
        .route("/api/knowledge/check", post(check_handler))
}

async fn status_handler(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<KnowledgeStatus> {
    let paths = KnowledgePaths::from_runtime_dir(&state.runtime_dir);
    Json(get_status(&paths))
}

/// `check` 与 `status` 当前实现一致；保留双端点是为了和 Python 的语义对齐——
/// `status` 是只读读取，`check` 暗示"主动复核"（后续 stage 会接入耗时校验如版本比对）。
async fn check_handler(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<KnowledgeStatus> {
    let paths = KnowledgePaths::from_runtime_dir(&state.runtime_dir);
    Json(get_status(&paths))
}
